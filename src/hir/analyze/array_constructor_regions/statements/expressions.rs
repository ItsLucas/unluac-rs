//! 按原始寄存器顺序恢复一条源码表达式及其唯一消费位置。
//!
//! 输入依赖完整源码帧中可读的低槽绑定和每条原始指令；弹出临时表达式只改变表示，
//! 不能抹去原始物理写。例如 `obj.method(x)` 必须保留 callee 读取与参数 MOVE 的顺序。
//! LOADK 字面量和 MOVE 裸绑定不能伪装成计算临时值折进算术或查表；未知写回、
//! 重复消费、会改变临时栈的计算与调用形状均使整份事务失败。

use super::*;
use crate::hir::common::{HirAssign, HirBinaryExpr, HirTableAccess};

pub(super) enum ExpressionEnd {
    Ignored(HirCallExpr),
    Iterator(HirCallExpr),
    FixedResults {
        call: HirCallExpr,
        definition: usize,
        width: usize,
    },
    Assigned(HirAssign),
    NumericHeader([HirExpr; 3]),
    ConditionValues(Vec<HirExpr>),
    Value {
        value: HirExpr,
        definition: usize,
    },
}

// This stack is built only from original instructions: a bare LocalRef/ParamRef
// here came from MOVE, and a literal came from a materializing load. Folding
// either into GETTABLE/arithmetic would let Lua use the source/RK operand directly
// and erase that physical write. Computed producers retain their destination.
pub(super) fn is_materialized_expression(value: &HirExpr) -> bool {
    matches!(
        value,
        HirExpr::GlobalRef(_)
            | HirExpr::UpvalueRef(_)
            | HirExpr::TableAccess(_)
            | HirExpr::Call(_)
            | HirExpr::Binary(_)
            | HirExpr::Unary(_)
            | HirExpr::TableConstructor(_)
    )
}

impl StatementParser<'_, '_> {
    pub(super) fn local_initializer(
        &mut self,
        start: usize,
        end: usize,
        active: &Frame,
        call_prefix: bool,
    ) -> Option<(HirExpr, usize, usize)> {
        if !matches!(
            self.lowering.proto.instrs.get(start),
            Some(
                LowInstr::LoadConst(_)
                    | LowInstr::LoadBool(_)
                    | LowInstr::GetTable(_)
                    | LowInstr::GetUpvalue(_)
                    | LowInstr::UnaryOp(_)
                    | LowInstr::BinaryOp(_)
            )
        ) {
            return None;
        }
        let mut cursor = start;
        let result = self.expression_mode(&mut cursor, end, active, !call_prefix, call_prefix)?;
        self.expression_origin = Some((start, cursor));
        match result {
            ExpressionEnd::Value { value, definition }
                if !call_prefix || matches!(value, HirExpr::Call(_)) =>
            {
                Some((value, definition, cursor))
            }
            _ => None,
        }
    }
    pub(super) fn expression(
        &mut self,
        pc: &mut usize,
        end: usize,
        active: &Frame,
    ) -> Option<ExpressionEnd> {
        let start = *pc;
        let result = self.expression_mode(pc, end, active, false, false)?;
        self.expression_origin = Some((start, *pc));
        if let ExpressionEnd::Assigned(assign) = &result
            && !self.assignment_constant_order(start, *pc, assign)
        {
            return None;
        }
        Some(result)
    }

    fn expression_mode(
        &mut self,
        pc: &mut usize,
        end: usize,
        active: &Frame,
        local_prefix: bool,
        call_prefix: bool,
    ) -> Option<ExpressionEnd> {
        let start = *pc;
        let block = self.lowering.cfg.instr_to_block[*pc];
        let base = active.len();
        let mut pending = Vec::<HirExpr>::new();
        let mut definition = *pc;
        while *pc < end {
            if matches!(
                self.lowering.proto.instrs.get(*pc),
                Some(LowInstr::Branch(_))
            ) {
                self.expression_origin = Some((start, *pc));
            }
            if pending.len() <= 2
                && pending.iter().all(is_materialized_expression)
                && matches!(
                    self.lowering.proto.instrs.get(*pc),
                    Some(LowInstr::Branch(_))
                )
                && let Some(value) = self.materialized_boolean(
                    pc,
                    end,
                    active,
                    (!pending.is_empty()).then_some(pending.as_slice()),
                    !local_prefix && !call_prefix,
                )
            {
                return Some(value);
            }
            if pending.len() == 2
                && pending.iter().all(is_materialized_expression)
                && matches!(
                    self.lowering.proto.instrs.get(*pc),
                    Some(LowInstr::Branch(_))
                )
            {
                return Some(ExpressionEnd::ConditionValues(pending));
            }
            if local_prefix
                && pending.len() == 1
                && !matches!(self.lowering.proto.instrs.get(*pc),
                    Some(LowInstr::GetTable(get)) if get.dst.index() == base && get.base == AccessBase::Reg(get.dst))
                && !matches!(self.lowering.proto.instrs.get(*pc),
                    Some(LowInstr::BinaryOp(binary)) if binary.dst.index() == base)
                && !matches!(self.lowering.proto.instrs.get(*pc),
                    Some(LowInstr::UnaryOp(unary)) if unary.dst.index() == base && unary.src.index() == base)
                && !matches!(self.lowering.proto.instrs.get(*pc),
                    Some(LowInstr::Call(call)) if call.callee.index() == base)
            {
                return Some(ExpressionEnd::Value {
                    value: pending.pop()?,
                    definition,
                });
            }
            if pending.len() == 1
                && matches!(
                    self.lowering.proto.instrs.get(*pc),
                    Some(LowInstr::Branch(_) | LowInstr::Return(_))
                )
            {
                return Some(ExpressionEnd::Value {
                    value: pending.pop()?,
                    definition,
                });
            }
            if self.lowering.cfg.instr_to_block[*pc] != block {
                return None;
            }
            let next = base + pending.len();
            match self.lowering.proto.instrs.get(*pc)?.clone() {
                LowInstr::LoadConst(load) if pending.is_empty() && load.dst.index() < base => {
                    let value = expr_for_const(self.lowering.proto, load.value);
                    *pc += 1;
                    return Some(ExpressionEnd::Assigned(
                        self.local_assignment(load.dst, value, active)?,
                    ));
                }
                LowInstr::LoadBool(load) if pending.is_empty() && load.dst.index() < base => {
                    *pc += 1;
                    return Some(ExpressionEnd::Assigned(self.local_assignment(
                        load.dst,
                        HirExpr::Boolean(load.value),
                        active,
                    )?));
                }
                LowInstr::Move(mov)
                    if pending.is_empty() && mov.dst.index() < base && mov.src.index() < base =>
                {
                    let value = active[mov.src.index()].value.clone()?;
                    *pc += 1;
                    return Some(ExpressionEnd::Assigned(
                        self.local_assignment(mov.dst, value, active)?,
                    ));
                }
                LowInstr::NumericForInit(init)
                    if pending.len() == 3
                        && init.index.index() == base
                        && init.limit.index() == base + 1
                        && init.step.index() == base + 2
                        && init.binding.index() == base + 3 =>
                {
                    return Some(ExpressionEnd::NumericHeader(pending.try_into().ok()?));
                }
                LowInstr::Move(mov) if mov.dst.index() == next && mov.src.index() < base => {
                    pending.push(active.get(mov.src.index())?.value.clone()?);
                }
                LowInstr::LoadConst(load) if load.dst.index() == next => {
                    let value = expr_for_const(self.lowering.proto, load.value);
                    if !matches!(
                        &value,
                        HirExpr::Nil
                            | HirExpr::Boolean(_)
                            | HirExpr::Integer(_)
                            | HirExpr::String(_)
                    ) && !matches!(&value, HirExpr::Number(number) if number.is_finite())
                    {
                        return None;
                    }
                    pending.push(value);
                }
                LowInstr::LoadBool(load) if load.dst.index() == next => {
                    pending.push(HirExpr::Boolean(load.value))
                }
                LowInstr::LoadNil(load) if load.dst.start.index() == next && load.dst.len > 0 => {
                    pending.extend(std::iter::repeat_n(HirExpr::Nil, load.dst.len));
                }
                LowInstr::GetUpvalue(get) if get.dst.index() == next => {
                    pending.push(self.scalar_rhs(*pc, active)?);
                }
                LowInstr::NewTable(seed) if seed.dst.index() == next => {
                    pending.push(self.table(pc, active)?);
                    continue;
                }
                LowInstr::GetTable(get) if get.kind == GetTableKind::Normal => {
                    if let AccessKey::Reg(key) = get.key
                        && key.index() >= base
                        && key.index() + 1 == next
                        && get.dst == key
                        && matches!(get.base, AccessBase::Reg(owner) if owner.index() < base)
                        && pending.last().is_some_and(is_materialized_expression)
                    {
                        let AccessBase::Reg(owner) = get.base else {
                            return None;
                        };
                        let key = pending.pop()?;
                        pending.push(HirExpr::TableAccess(Box::new(HirTableAccess {
                            base: active[owner.index()].value.clone()?,
                            key,
                        })));
                        *pc += 1;
                        continue;
                    }
                    let key = match get.key {
                        AccessKey::Const(key) if key.index() <= 255 => {
                            expr_for_const(self.lowering.proto, key)
                        }
                        AccessKey::Reg(reg) if reg.index() < base => {
                            active[reg.index()].value.clone()?
                        }
                        _ => return None,
                    };
                    let expression = match get.base {
                        AccessBase::Env
                            if (get.dst.index() == next
                                || pending.is_empty() && get.dst.index() < base)
                                && matches!(get.key, AccessKey::Const(_)) =>
                        {
                            self.scalar_rhs(*pc, active)?
                        }
                        AccessBase::Reg(reg)
                            if reg.index() < base
                                && (get.dst.index() == next
                                    || pending.is_empty() && get.dst.index() < base) =>
                        {
                            HirExpr::TableAccess(Box::new(HirTableAccess {
                                base: active.get(reg.index())?.value.clone()?,
                                key,
                            }))
                        }
                        AccessBase::Reg(reg)
                            if !pending.is_empty()
                                && reg.index() + 1 == next
                                && get.dst == reg
                                && pending.last().is_some_and(is_materialized_expression) =>
                        {
                            HirExpr::TableAccess(Box::new(HirTableAccess {
                                base: pending.pop()?,
                                key,
                            }))
                        }
                        _ => return None,
                    };
                    if get.dst.index() < base {
                        *pc += 1;
                        return Some(ExpressionEnd::Assigned(
                            self.local_assignment(get.dst, expression, active)?,
                        ));
                    }
                    pending.push(expression);
                }
                LowInstr::UnaryOp(unary) if unary.op == crate::transformer::UnaryOpKind::Length => {
                    let value = if unary.dst.index() == next && unary.src.index() < base {
                        active[unary.src.index()].value.clone()?
                    } else if !pending.is_empty()
                        && unary.dst == unary.src
                        && unary.dst.index() + 1 == next
                        && pending.last().is_some_and(is_materialized_expression)
                    {
                        pending.pop()?
                    } else {
                        return None;
                    };
                    pending.push(HirExpr::Unary(Box::new(HirUnaryExpr {
                        op: HirUnaryOpKind::Length,
                        expr: value,
                    })));
                }
                LowInstr::BinaryOp(binary) => {
                    if pending.len() >= 2
                        && binary.dst.index() + 2 == next
                        && binary.lhs == ValueOperand::Reg(binary.dst)
                        && binary.rhs == ValueOperand::Reg(Reg(binary.dst.index() + 1))
                        && pending[pending.len() - 2..]
                            .iter()
                            .all(is_materialized_expression)
                    {
                        let rhs = pending.pop()?;
                        let lhs = pending.pop()?;
                        pending.push(HirExpr::Binary(Box::new(HirBinaryExpr {
                            operand_order: None,
                            op: crate::hir::analyze::exprs::lower_binary_op(binary.op),
                            lhs,
                            rhs,
                        })));
                        *pc += 1;
                        continue;
                    }
                    let operand = |operand: ValueOperand, values: &[HirExpr]| match operand {
                        ValueOperand::Reg(reg) if reg.index() < base => {
                            active[reg.index()].value.clone()
                        }
                        ValueOperand::Reg(reg)
                            if !values.is_empty() && reg.index() + 1 == base + values.len() =>
                        {
                            values.last().cloned()
                        }
                        ValueOperand::Const(key) if key.index() <= 255 => {
                            let value = expr_for_const(self.lowering.proto, key);
                            matches!(&value, HirExpr::Integer(_))
                                .then_some(value.clone())
                                .or_else(|| {
                                    matches!(&value, HirExpr::Number(n) if n.is_finite())
                                        .then_some(value)
                                })
                        }
                        _ => None,
                    };
                    let lhs = operand(binary.lhs, &pending)?;
                    let rhs = operand(binary.rhs, &pending)?;
                    let runtime = |value: &HirExpr| {
                        !matches!(value, HirExpr::Integer(_) | HirExpr::Number(_))
                    };
                    if !runtime(&lhs) && !runtime(&rhs) {
                        return None;
                    }
                    if pending.len() == 1
                        && binary.dst.index() < base
                        && pending.first().is_some_and(is_materialized_expression)
                        && [binary.lhs, binary.rhs]
                            .iter()
                            .filter(|operand| **operand == ValueOperand::Reg(Reg(base)))
                            .count()
                            == 1
                    {
                        *pc += 1;
                        return Some(ExpressionEnd::Assigned(self.local_assignment(
                            binary.dst,
                            HirExpr::Binary(Box::new(HirBinaryExpr {
                                operand_order: None,
                                op: crate::hir::analyze::exprs::lower_binary_op(binary.op),
                                lhs,
                                rhs,
                            })),
                            active,
                        )?));
                    }
                    if pending.is_empty() && binary.dst.index() < base {
                        let HirExpr::LocalRef(local) = active[binary.dst.index()].value.as_ref()?
                        else {
                            return None;
                        };
                        *pc += 1;
                        return Some(ExpressionEnd::Assigned(HirAssign {
                            targets: vec![HirLValue::Local(*local)],
                            values: vec![HirExpr::Binary(Box::new(HirBinaryExpr {
                                operand_order: None,
                                op: crate::hir::analyze::exprs::lower_binary_op(binary.op),
                                lhs,
                                rhs,
                            }))]
                            .into(),
                        }));
                    }
                    let writes_top = !pending.is_empty() && binary.dst.index() + 1 == next;
                    let writes_next = binary.dst.index() == next;
                    if !writes_top && !writes_next {
                        return None;
                    }
                    if writes_top && !pending.last().is_some_and(is_materialized_expression) {
                        return None;
                    }
                    if writes_top
                        && [binary.lhs, binary.rhs]
                            .iter()
                            .filter(|operand| **operand == ValueOperand::Reg(binary.dst))
                            .count()
                            != 1
                    {
                        return None;
                    }
                    if writes_next
                        && [binary.lhs, binary.rhs].iter().any(
                            |value| matches!(value, ValueOperand::Reg(reg) if reg.index() >= base),
                        )
                    {
                        return None;
                    }
                    if writes_top {
                        pending.pop();
                    }
                    pending.push(HirExpr::Binary(Box::new(HirBinaryExpr {
                        operand_order: None,
                        op: crate::hir::analyze::exprs::lower_binary_op(binary.op),
                        lhs,
                        rhs,
                    })));
                }
                LowInstr::Concat(concat) => {
                    let first = concat.src.start.index().checked_sub(base)?;
                    if concat.src.len < 2
                        || first + concat.src.len != pending.len()
                        || matches!(pending.last(), Some(HirExpr::Binary(binary)) if binary.op == HirBinaryOpKind::Concat)
                    {
                        return None;
                    }
                    let value = concat_expr(pending.split_off(first));
                    let definition = *pc;
                    *pc += 1;
                    if concat.dst.index() < base && first == 0 {
                        let HirExpr::LocalRef(local) = active[concat.dst.index()].value.as_ref()?
                        else {
                            return None;
                        };
                        return Some(ExpressionEnd::Assigned(HirAssign {
                            targets: vec![HirLValue::Local(*local)],
                            values: vec![value].into(),
                        }));
                    }
                    if concat.dst != concat.src.start {
                        return None;
                    }
                    if first == 0 {
                        if matches!(self.lowering.proto.instrs.get(*pc), Some(LowInstr::SetTable(store))
                            if store.base == AccessBase::Env && store.value == ValueOperand::Reg(concat.dst))
                        {
                            pending.push(value);
                            continue;
                        }
                        return Some(ExpressionEnd::Value { value, definition });
                    }
                    pending.push(value);
                    continue;
                }
                LowInstr::SetTable(store) if store.kind == SetTableKind::Normal => {
                    if store.base == AccessBase::Env || pending.is_empty() {
                        let assign = self.direct_store(*pc, active, &mut pending, store)?;
                        *pc += 1;
                        return Some(ExpressionEnd::Assigned(assign));
                    }
                    let AccessBase::Reg(owner) = store.base else {
                        return None;
                    };
                    if owner.index() >= base {
                        return None;
                    }
                    if !pending.last().is_some_and(is_materialized_expression) {
                        return None;
                    }
                    let owner = active[owner.index()].value.clone()?;
                    let key = match store.key {
                        AccessKey::Reg(key)
                            if key.index() == base
                                && pending.len() == 2
                                && pending.first().is_some_and(is_materialized_expression) =>
                        {
                            pending.remove(0)
                        }
                        AccessKey::Reg(key) if key.index() < base && pending.len() == 1 => {
                            active[key.index()].value.clone()?
                        }
                        AccessKey::Const(key) if pending.len() == 1 => {
                            expr_for_const(self.lowering.proto, key)
                        }
                        _ => return None,
                    };
                    let value_reg = match store.key {
                        AccessKey::Reg(key) if key.index() == base => base + 1,
                        _ => base,
                    };
                    if store.value != ValueOperand::Reg(Reg(value_reg)) {
                        return None;
                    }
                    *pc += 1;
                    return Some(ExpressionEnd::Assigned(HirAssign {
                        targets: vec![HirLValue::TableAccess(Box::new(HirTableAccess {
                            base: owner,
                            key,
                        }))],
                        values: vec![pending.pop()?].into(),
                    }));
                }
                LowInstr::Call(call)
                    if call.kind == CallKind::Normal && call.method_name.is_none() =>
                {
                    if matches!(call.results, ResultPack::Open(_)) {
                        return open_packs::open_call_statement(
                            self,
                            pc,
                            end,
                            active,
                            &mut pending,
                            call,
                        );
                    }
                    let ValuePack::Fixed(args) = call.args else {
                        return None;
                    };
                    let first = call.callee.index().checked_sub(base)?;
                    if args.start.index() != call.callee.index() + 1
                        || first + args.len + 1 != pending.len()
                    {
                        return None;
                    }
                    let mut operands = pending.split_off(first).into_iter();
                    let expression = HirCallExpr {
                        callee: operands.next()?,
                        args: operands.collect::<Vec<_>>().into(),
                        method: false,
                        fastcall: None,
                        method_name: None,
                    };
                    let call_pc = *pc;
                    *pc += 1;
                    match call.results {
                        ResultPack::Ignore if first == 0 => {
                            return Some(ExpressionEnd::Ignored(expression));
                        }
                        ResultPack::Fixed(results)
                            if results.start == call.callee && results.len == 1 =>
                        {
                            if first == 0 {
                                if call_prefix {
                                    return Some(ExpressionEnd::Value {
                                        value: HirExpr::Call(Box::new(expression)),
                                        definition: call_pc,
                                    });
                                }
                                if let Some(LowInstr::Move(mov)) =
                                    self.lowering.proto.instrs.get(*pc)
                                    && mov.src == call.callee
                                    && mov.dst.index() < base
                                    && let Some(HirExpr::LocalRef(local)) =
                                        &active[mov.dst.index()].value
                                {
                                    *pc += 1;
                                    return Some(ExpressionEnd::Assigned(HirAssign {
                                        targets: vec![HirLValue::Local(*local)],
                                        values: vec![HirExpr::Call(Box::new(expression))].into(),
                                    }));
                                }
                                if matches!(self.lowering.proto.instrs.get(*pc),
                                    Some(LowInstr::GetTable(get)) if get.dst == call.callee && get.base == AccessBase::Reg(call.callee))
                                    || matches!(self.lowering.proto.instrs.get(*pc),
                                        Some(LowInstr::UnaryOp(unary)) if unary.dst == call.callee && unary.src == call.callee)
                                    || matches!(self.lowering.proto.instrs.get(*pc),
                                        Some(LowInstr::BinaryOp(binary)) if binary.dst == call.callee || binary.dst.index() < base)
                                    || matches!(self.lowering.proto.instrs.get(*pc),
                                        Some(LowInstr::SetTable(store)) if store.base == AccessBase::Env && store.value == ValueOperand::Reg(call.callee))
                                    || matches!(
                                        self.lowering.proto.instrs.get(*pc),
                                        Some(LowInstr::Branch(_))
                                    )
                                    || (!local_prefix
                                        && match self.lowering.proto.instrs.get(*pc) {
                                            Some(LowInstr::GetTable(get)) => {
                                                get.dst.index() == base + 1
                                            }
                                            Some(LowInstr::GetUpvalue(get)) => {
                                                get.dst.index() == base + 1
                                            }
                                            Some(LowInstr::Move(mov)) => {
                                                mov.dst.index() == base + 1
                                                    && mov.src.index() < base
                                            }
                                            Some(LowInstr::LoadConst(load)) => {
                                                load.dst.index() == base + 1
                                            }
                                            Some(LowInstr::LoadBool(load)) => {
                                                load.dst.index() == base + 1
                                            }
                                            _ => false,
                                        })
                                {
                                    definition = call_pc;
                                    pending.push(HirExpr::Call(Box::new(expression)));
                                    continue;
                                }
                                return Some(ExpressionEnd::Value {
                                    value: HirExpr::Call(Box::new(expression)),
                                    definition: call_pc,
                                });
                            }
                            pending.push(HirExpr::Call(Box::new(expression)));
                            continue;
                        }
                        ResultPack::Fixed(results)
                            if first == 0 && results.start == call.callee && results.len == 3 =>
                        {
                            return Some(ExpressionEnd::Iterator(expression));
                        }
                        ResultPack::Fixed(results)
                            if first == 0 && results.start == call.callee && results.len >= 2 =>
                        {
                            return Some(ExpressionEnd::FixedResults {
                                call: expression,
                                definition: call_pc,
                                width: results.len,
                            });
                        }
                        _ => return None,
                    }
                }
                _ => return None,
            }
            if pending.len() == 1 {
                definition = *pc;
            }
            *pc += 1;
        }
        None
    }

    fn scalar_rhs(&self, pc: usize, active: &Frame) -> Option<HirExpr> {
        let statements = lower_regular_instr(
            self.lowering,
            self.lowering.cfg.instr_to_block[pc],
            InstrRef(pc),
            &self.lowering.proto.instrs[pc],
        )?;
        let [statement] = statements.as_slice() else {
            return None;
        };
        let values = match statement {
            HirStmt::Assign(assign) => &assign.values,
            HirStmt::LocalDecl(decl) => &decl.values,
            _ => return None,
        };
        let ([value], None) = (values.fixed.as_slice(), &values.tail) else {
            return None;
        };
        let mut value = value.clone();
        frames::remap_expression(&mut value, active)?;
        Some(value)
    }
}
