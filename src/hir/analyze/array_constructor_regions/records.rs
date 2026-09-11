//! 用原始 LIR 的临时槽栈恢复纯记录表，保留每个字段键和值的求值及写入顺序。
//!
//! 大于 RK 容量的字符串键和池满后的有限数值键先占一槽，字段表达式从后一槽开始；
//! 常量来源与运行时值分开保存，避免把 LOADK 键或大池 LOADBOOL／LOADNIL 当作任意表达式。
//! `NEWTABLE child; ...; SETTABLE owner[key] = child` 可递归收为 `key = {...}`，
//! 但必须证明子表在正确槽上完整闭合且立即写回，不能吃掉父数组的下一兄弟项。
//! 本模块只证明字段表达式；入口帧、SSA 使用范围与根逃逸仍由外层区间事务负责。
//! 固定全局调用作键时，保留一个单值结果槽直到随后子表写回；闭包字段只能捕获区间外
//! 已存在的绑定。重复键仍按写次数计分配，不扩展任意动态键、开放调用或混合数组字段。
//! 字段值中的 `GETGLOBAL math; GETTABLE cos; ADD arg; CALL; MUL; ADD; SETTABLE`
//! 可恢复为 `x = x + scale * math.cos(angle + delta)`：原始连续栈决定 callee、参数和
//! 算术的消费位置，低槽身份由父帧证书提供。MOVE 只可作为调用 callee／参数，不能
//! 冒充计算临时值折入算术或字段写入；直接 SETTABLE 低槽则保留在该写入时读取。

use super::*;
use crate::hir::common::{HirRecordField, HirTableAccess, HirTableKey};
use crate::transformer::{CaptureSource, ClosureCreation, ConstRef, NewTableInstr};

const MAX_RK_INDEX: usize = 255;

struct RecordSlot {
    expr: HirExpr,
    source: RecordSource,
}

enum RecordSource {
    Computed,
    CallKey,
    Loaded(ConstRef),
    Immediate,
    // 参数或 callee 的 MOVE 在 CALL 中仍会物化；用于算术／SETTABLE 会被 Lua
    // 编译为直接低槽操作数，丢失原始写入，因此必须与 Computed 分开。
    Copied,
    // 小池 LOADK 可以是固定调用参数，但尚未证明它能作 RK 字段值或运算数。
    Argument,
}

impl RecordSlot {
    fn computed(expr: HirExpr) -> Self {
        Self {
            expr,
            source: RecordSource::Computed,
        }
    }

    fn is_computed(&self) -> bool {
        matches!(self.source, RecordSource::Computed)
    }

    fn is_large_string(&self) -> bool {
        matches!(self.source, RecordSource::Loaded(key) if key.index() > MAX_RK_INDEX)
            && matches!(self.expr, HirExpr::String(_))
    }

    fn is_loaded_number(&self) -> bool {
        matches!(self.source, RecordSource::Loaded(_)) && is_finite_number(&self.expr)
    }

    fn is_materialized_key(&self) -> bool {
        // 数值 LOADK 只有此前已池满或自身索引越界时才能入栈；即使复用低索引
        // 常量，recfield 的 VKNUM 也不能退回 RK。字符串 VK 则只看自身索引。
        self.is_large_string() || self.is_loaded_number() || self.is_call_key()
    }

    fn is_call_key(&self) -> bool {
        matches!(self.source, RecordSource::CallKey)
    }
}

fn is_finite_number(expr: &HirExpr) -> bool {
    // inf／NaN 的源码拼写会生成除法和额外槽写入，不再对应原始的单条 LOADK。
    match expr {
        HirExpr::Integer(_) => true,
        HirExpr::Number(value) => value.is_finite(),
        _ => false,
    }
}

fn reaches_rk_limit(instr: &LowInstr) -> bool {
    let key_reaches_limit =
        |key| matches!(key, AccessKey::Const(key) if key.index() >= MAX_RK_INDEX);
    let value_reaches_limit =
        |value| matches!(value, ValueOperand::Const(value) if value.index() >= MAX_RK_INDEX);
    match instr {
        LowInstr::LoadConst(load) => load.value.index() >= MAX_RK_INDEX,
        LowInstr::GetTable(get) => key_reaches_limit(get.key),
        LowInstr::SetTable(store) => {
            key_reaches_limit(store.key) || value_reaches_limit(store.value)
        }
        LowInstr::BinaryOp(binary) => {
            value_reaches_limit(binary.lhs) || value_reaches_limit(binary.rhs)
        }
        _ => false,
    }
}

impl ArrayParser<'_, '_> {
    fn low_record_value(&self, reg: Reg) -> Option<HirExpr> {
        if reg.index() >= self.root.index() {
            return None;
        }
        let value = crate::hir::analyze::exprs::expr_for_reg_use(
            self.lowering,
            self.block,
            InstrRef(self.cursor),
            reg,
        );
        matches!(
            value,
            HirExpr::ParamRef(_) | HirExpr::LocalRef(_) | HirExpr::TempRef(_)
        )
        .then_some(value)
    }

    fn low_record_binary(
        &self,
        binary: &crate::transformer::BinaryOpInstr,
        scratch: Reg,
        values: &mut Vec<RecordSlot>,
    ) -> Option<bool> {
        let low =
            |operand| matches!(operand, ValueOperand::Reg(reg) if reg.index() < self.root.index());
        if !low(binary.lhs) && !low(binary.rhs) {
            return Some(false);
        }
        let next = scratch.index() + values.len();
        let low_operand = |operand| match operand {
            ValueOperand::Reg(reg) => self.low_record_value(reg),
            _ => None,
        };
        let (lhs, rhs) = if binary.dst.index() == next {
            // 两个既有低槽直接在运算指令中读取；不能先替它们造 MOVE，也不能
            // 将读时点移到前面的 callee lookup 或 CALL 之前。
            (low_operand(binary.lhs)?, low_operand(binary.rhs)?)
        } else if binary.dst.index() + 1 == next
            && values.last().is_some_and(RecordSlot::is_computed)
            && [binary.lhs, binary.rhs]
                .into_iter()
                .filter(|operand| *operand == ValueOperand::Reg(binary.dst))
                .count()
                == 1
        {
            let computed = values.pop()?.expr;
            if binary.lhs == ValueOperand::Reg(binary.dst) {
                (computed, low_operand(binary.rhs)?)
            } else {
                (low_operand(binary.lhs)?, computed)
            }
        } else {
            return None;
        };
        values.push(RecordSlot::computed(HirExpr::Binary(Box::new(
            crate::hir::common::HirBinaryExpr {
                operand_order: None,
                op: crate::hir::analyze::exprs::lower_binary_op(binary.op),
                lhs,
                rhs,
            },
        ))));
        Some(true)
    }

    fn nested_record_value(
        &mut self,
        owner: Reg,
        values: &[RecordSlot],
        depth: usize,
    ) -> Option<(HirExpr, bool)> {
        if !values.is_empty() && !matches!(values, [key] if key.is_materialized_key()) {
            return None;
        }
        let LowInstr::NewTable(child) = *self.instruction()? else {
            return None;
        };
        if child.dst.index() != owner.index() + values.len() + 1 {
            return None;
        }
        // recfield 在生成子表前先 intern 父键。键为 RK 时，字节码直到子表闭合后的
        // SETTABLE 才携带它；这一明确父消费提供子表入口池状态，不能借用任意后缀常量。
        let (store_pc, store) = self
            .lowering
            .proto
            .instrs
            .iter()
            .enumerate()
            .skip(self.cursor + 1)
            .take(MAX_REGION_INSTRS)
            .take_while(|(pc, _)| self.lowering.cfg.instr_to_block.get(*pc) == Some(&self.block))
            .find_map(|(pc, instruction)| match instruction {
                LowInstr::SetTable(store) if store.base == AccessBase::Reg(owner) => {
                    Some((pc, store))
                }
                _ => None,
            })?;
        if store.kind != SetTableKind::Normal || store.value != ValueOperand::Reg(child.dst) {
            return None;
        }
        if matches!(values, [key] if key.is_call_key())
            && store.key != AccessKey::Reg(Reg(owner.index() + 1))
        {
            return None;
        }
        // 同一槽上的 NEWTABLE 也可能是父数组的下一条记录。只有紧随的父字段写
        // 验证成功才推进游标；圆整后的 hash hint 不能独自决定记录在哪里结束。
        let mut parser = self.clone();
        parser.rk_pool_full |=
            matches!(store.key, AccessKey::Const(key) if key.index() >= MAX_RK_INDEX);
        let expression = parser.table(depth + 1)?;
        if parser.cursor != store_pc
            || !matches!(parser.instruction()?, LowInstr::SetTable(store)
            if store.kind == SetTableKind::Normal
                && store.base == AccessBase::Reg(owner)
                && store.value == ValueOperand::Reg(child.dst))
        {
            return None;
        }
        let fills_pool = parser.rk_pool_full
            || self.lowering.proto.instrs[self.cursor..parser.cursor]
                .iter()
                .any(reaches_rk_limit);
        *self = parser;
        Some((expression, fills_pool))
    }

    fn fixed_record_key_call(
        &mut self,
        owner: Reg,
        values: &[RecordSlot],
    ) -> Option<(RecordSlot, bool)> {
        if !values.is_empty() && !matches!(values, [key] if key.is_materialized_key()) {
            return None;
        }
        let LowInstr::GetTable(get) = *self.instruction()? else {
            return None;
        };
        if get.kind != GetTableKind::Normal
            || get.base != AccessBase::Env
            || !matches!(get.key, AccessKey::Const(_))
            || get.dst.index() != owner.index() + values.len() + 1
        {
            return None;
        }
        let callee = self.scalar_value()?;
        let mut parser = self.clone();
        let mut full_pool = reaches_rk_limit(parser.instruction()?);
        parser.cursor += 1;
        let mut args = Vec::new();
        while let LowInstr::LoadConst(load) = *parser.instruction()? {
            if load.dst.index() != get.dst.index() + args.len() + 1 {
                return None;
            }
            let value = expr_for_const(parser.lowering.proto, load.value);
            if !matches!(value, HirExpr::String(_)) && !is_finite_number(&value) {
                return None;
            }
            args.push(value);
            full_pool |= reaches_rk_limit(parser.instruction()?);
            parser.cursor += 1;
        }
        let LowInstr::Call(call) = *parser.instruction()? else {
            return None;
        };
        if call.kind != CallKind::Normal
            || call.method_name.is_some()
            || call.callee != get.dst
            || !matches!(call.args, ValuePack::Fixed(arguments)
                if arguments.start.index() == get.dst.index() + 1 && arguments.len == args.len())
            || !matches!(call.results, ResultPack::Fixed(results)
                if results.start == get.dst && results.len == 1)
        {
            return None;
        }
        parser.cursor += 1;
        let source = match parser.instruction()? {
            // 动态键只开放这条完整形状：固定单值调用、紧邻的子表分配、再由
            // nested_record_value 证明同一键槽保持到父 SETTABLE；不能借它放行
            // 任意 computed 临时值，也不能把调用结果变成开放包。
            LowInstr::NewTable(child)
                if values.is_empty() && child.dst.index() == get.dst.index() + 1 =>
            {
                RecordSource::CallKey
            }
            _ => return None,
        };
        self.cursor = parser.cursor;
        self.has_observable_producer = true;
        Some((
            RecordSlot {
                expr: HirExpr::Call(Box::new(HirCallExpr {
                    callee,
                    args: args.into(),
                    method: false,
                    fastcall: None,
                    method_name: None,
                })),
                source,
            },
            full_pool,
        ))
    }

    fn materialized_primitive_store(
        &self,
        owner: Reg,
        dst: Reg,
        values: &[RecordSlot],
        full_pool: bool,
    ) -> bool {
        let Some(LowInstr::SetTable(store)) = self.lowering.proto.instrs.get(self.cursor + 1)
        else {
            return false;
        };
        if store.kind != SetTableKind::Normal
            || store.base != AccessBase::Reg(owner)
            || store.value != ValueOperand::Reg(dst)
        {
            return false;
        }
        match store.key {
            // recfield interns the key before compiling its value. Index 255
            // therefore fills the RK pool even if no earlier instruction did.
            AccessKey::Const(key) => {
                values.is_empty()
                    && dst.index() == owner.index() + 1
                    && (full_pool || key.index() >= MAX_RK_INDEX)
            }
            AccessKey::Reg(key) => {
                matches!(values, [value]
                    if value.is_materialized_key() && (!value.is_call_key() || full_pool))
                    && key.index() == owner.index() + 1
                    && dst.index() == key.index() + 1
            }
            AccessKey::Integer(_) => false,
        }
    }

    fn materialized_numeric_rhs(&self, dst: Reg, values: &[RecordSlot]) -> bool {
        // 只恢复运行时左值之后紧邻加载的数值右操作数。左值保证不会被 Lua 常量
        // 折叠；反向寄存器顺序、纯字面量运算及其它常量用途仍不能借此放行。
        values.last().is_some_and(RecordSlot::is_computed)
            && matches!(self.lowering.proto.instrs.get(self.cursor + 1),
                Some(LowInstr::BinaryOp(binary))
                    if binary.dst.index() + 1 == dst.index()
                        && binary.lhs == ValueOperand::Reg(binary.dst)
                        && binary.rhs == ValueOperand::Reg(dst))
    }

    pub(super) fn record_table(&mut self, seed: NewTableInstr, depth: usize) -> Option<HirExpr> {
        let allocation = seed.lua51_allocation?;
        let scratch = Reg(seed.dst.index() + 1);
        let mut values: Vec<RecordSlot> = Vec::new();
        let mut fields = Vec::new();
        // 只使用已发射操作数和明确父字段的先入池键。最终常量池大小还包含当前
        // 构造之后的无关常量，不能拿来证明某一时点的 LOADK 与 RK 选择。
        let mut full_pool = self.rk_pool_full
            || self.lowering.proto.instrs[..self.cursor]
                .iter()
                .any(reaches_rk_limit);
        let mut field_pool_full = full_pool;
        self.cursor += 1;
        loop {
            // 顶层捕获记录后可接局部常量：复用已出现池项，或用整个小池上界证明
            // 有限数值从未需要字段 LOADK，并紧接另一个已捕获表声明。这里不会用
            // 最终池大小假定更早已满池；自身定义必须被捕获，后继帧仍由前缀另证。
            if depth == 0
                && values.is_empty()
                && !fields.is_empty()
                && allocation == Lua51TableAllocation::from_field_counts(0, fields.len())
                && let LowInstr::LoadConst(load) = self.instruction()?
                && load.dst == scratch
                && self.lowering.dataflow.instr_defs[self.cursor].len() == 1
                && definition_is_captured(self.lowering, self.lowering.dataflow.instr_defs[self.cursor][0])
                && !matches!(self.lowering.proto.instrs.get(self.cursor + 1),
                    Some(LowInstr::SetTable(store)) if store.base == AccessBase::Reg(seed.dst))
                && (self.lowering.proto.instrs[self.start..self.cursor].iter().any(|instruction| {
                    match instruction {
                        LowInstr::LoadConst(previous) => previous.value == load.value,
                        LowInstr::GetTable(get) => get.key == AccessKey::Const(load.value),
                        LowInstr::SetTable(store) => store.key == AccessKey::Const(load.value)
                            || store.value == ValueOperand::Const(load.value),
                        LowInstr::BinaryOp(binary) => binary.lhs == ValueOperand::Const(load.value)
                            || binary.rhs == ValueOperand::Const(load.value),
                        _ => false,
                    }
                }) || (self.lowering.proto.constants.common.literals.len() <= 255
                    && is_finite_number(&expr_for_const(self.lowering.proto, load.value))
                    && matches!(self.lowering.proto.instrs.get(self.cursor + 1),
                        Some(LowInstr::NewTable(next)) if next.dst.index() == scratch.index() + 1)
                    && self.lowering.dataflow.instr_defs.get(self.cursor + 1).is_some_and(|defs|
                        matches!(defs.as_slice(), [def] if definition_is_captured(self.lowering, *def)))))
            {
                break;
            }
            // 分配容量已不可能容纳新字段时，不递归试探下一条兄弟记录。
            // 这里仅排除候选；实际完成仍须通过写入边界和精确字段计数校验。
            if Lua51TableAllocation::from_field_counts(0, fields.len() + 1).hash_hint
                <= allocation.hash_hint
                && let Some((table, table_fills_pool)) =
                    self.nested_record_value(seed.dst, &values, depth)
            {
                values.push(RecordSlot::computed(table));
                full_pool |= table_fills_pool;
                continue;
            }
            if let Some((call, call_fills_pool)) = self.fixed_record_key_call(seed.dst, &values) {
                values.push(call);
                full_pool |= call_fills_pool;
                continue;
            }
            let instruction = self.instruction()?.clone();
            let reaches_limit = reaches_rk_limit(&instruction);
            match instruction {
                LowInstr::LoadConst(load) if load.dst.index() == scratch.index() + values.len() => {
                    let expr = expr_for_const(self.lowering.proto, load.value);
                    let numeric_rhs = is_finite_number(&expr)
                        && (full_pool || load.value.index() > MAX_RK_INDEX)
                        && self.materialized_numeric_rhs(load.dst, &values);
                    let numeric_key = values.is_empty()
                        && is_finite_number(&expr)
                        && (full_pool || load.value.index() > MAX_RK_INDEX);
                    if !matches!(expr, HirExpr::String(_)) && !is_finite_number(&expr) {
                        return None;
                    }
                    let materialized = load.value.index() > MAX_RK_INDEX
                        || numeric_rhs
                        || numeric_key
                        || (!matches!(expr, HirExpr::String(_))
                            && self.materialized_primitive_store(
                                seed.dst, load.dst, &values, full_pool,
                            ));
                    values.push(RecordSlot {
                        expr,
                        source: if materialized {
                            RecordSource::Loaded(load.value)
                        } else {
                            RecordSource::Argument
                        },
                    });
                }
                LowInstr::Move(mov)
                    if mov.dst.index() == scratch.index() + values.len()
                        && mov.src.index() < self.root.index() =>
                {
                    values.push(RecordSlot {
                        expr: self.low_record_value(mov.src)?,
                        source: RecordSource::Copied,
                    });
                }
                LowInstr::Call(call) => {
                    if call.kind != CallKind::Normal || call.method_name.is_some() {
                        return None;
                    }
                    let ValuePack::Fixed(args) = call.args else {
                        return None;
                    };
                    let first = call.callee.index().checked_sub(scratch.index())?;
                    if args.start.index() != call.callee.index() + 1
                        || first + args.len + 1 != values.len()
                        || !matches!(call.results, ResultPack::Fixed(results) if results.start == call.callee && results.len == 1)
                        || !matches!(
                            values.get(first)?.source,
                            RecordSource::Computed | RecordSource::Copied
                        )
                        || (first != 0 && !(first == 1 && values[0].is_materialized_key()))
                    {
                        return None;
                    }
                    let mut operands = values.split_off(first).into_iter();
                    values.push(RecordSlot::computed(HirExpr::Call(Box::new(HirCallExpr {
                        callee: operands.next()?.expr,
                        args: operands.map(|slot| slot.expr).collect::<Vec<_>>().into(),
                        method: false,
                        fastcall: None,
                        method_name: None,
                    }))));
                    self.has_observable_producer = true;
                }
                LowInstr::LoadBool(load)
                    if self
                        .materialized_primitive_store(seed.dst, load.dst, &values, full_pool) =>
                {
                    values.push(RecordSlot {
                        expr: HirExpr::Boolean(load.value),
                        source: RecordSource::Immediate,
                    });
                }
                LowInstr::LoadNil(load)
                    if load.dst.len == 1
                        && self.materialized_primitive_store(
                            seed.dst,
                            load.dst.start,
                            &values,
                            full_pool,
                        ) =>
                {
                    values.push(RecordSlot {
                        expr: HirExpr::Nil,
                        source: RecordSource::Immediate,
                    });
                }
                LowInstr::Closure(closure)
                    if closure.creation == ClosureCreation::Fresh
                        && closure.dst.index() == scratch.index() + values.len()
                        && (values.is_empty()
                            || matches!(values.as_slice(), [key] if key.is_materialized_key()))
                        && closure.captures.iter().all(|capture| match capture.source {
                            CaptureSource::ByReference(reg) | CaptureSource::ByValue(reg) => {
                                reg.index() < self.root.index()
                            }
                            CaptureSource::Upvalue(_) => true,
                        })
                        && matches!(self.lowering.proto.instrs.get(self.cursor + 1),
                            Some(LowInstr::SetTable(store))
                                if store.kind == SetTableKind::Normal
                                    && store.base == AccessBase::Reg(seed.dst)
                                    && store.value == ValueOperand::Reg(closure.dst)) =>
                {
                    // 不移动闭包创建与其 capture：区间内产生的任何槽都不能被它
                    // 捕获，scalar lowering 还要求既有绑定不需要额外声明或 barrier。
                    let value = self.scalar_value()?;
                    if !matches!(value, HirExpr::Closure(_)) {
                        return None;
                    }
                    values.push(RecordSlot::computed(value));
                    self.has_observable_producer = true;
                }
                LowInstr::GetTable(get) if get.kind == GetTableKind::Normal => {
                    let next = scratch.index() + values.len();
                    match get.key {
                        AccessKey::Const(key) => {
                            if get.dst.index() == next
                                && match get.base {
                                    AccessBase::Env => true,
                                    AccessBase::Reg(base) => base.index() < self.root.index(),
                                    _ => false,
                                }
                            {
                                values.push(RecordSlot::computed(self.scalar_value()?));
                            } else if !values.is_empty()
                                && get.dst.index() + 1 == next
                                && get.base == AccessBase::Reg(get.dst)
                                && values.last()?.is_computed()
                            {
                                let previous = values.pop()?.expr;
                                values.push(RecordSlot::computed(HirExpr::TableAccess(Box::new(
                                    HirTableAccess {
                                        base: previous,
                                        key: expr_for_const(self.lowering.proto, key),
                                    },
                                ))));
                            } else {
                                return None;
                            }
                        }
                        AccessKey::Reg(key)
                            if key.index() + 1 == next && values.last()?.is_large_string() =>
                        {
                            let key_expr = values.pop()?.expr;
                            let base = match get.base {
                                AccessBase::Reg(base)
                                    if base == get.dst
                                        && base.index() + 2 == next
                                        && values.last()?.is_computed() =>
                                {
                                    values.pop()?.expr
                                }
                                AccessBase::Reg(base)
                                    if base.index() < self.root.index() && get.dst == key =>
                                {
                                    let HirExpr::TableAccess(access) = self.scalar_value()? else {
                                        return None;
                                    };
                                    access.base
                                }
                                _ => return None,
                            };
                            values.push(RecordSlot::computed(HirExpr::TableAccess(Box::new(
                                HirTableAccess {
                                    base,
                                    key: key_expr,
                                },
                            ))));
                        }
                        _ => return None,
                    }
                    self.has_observable_producer = true;
                }
                LowInstr::BinaryOp(binary) => {
                    if self.low_record_binary(&binary, scratch, &mut values)? {
                        self.has_observable_producer = true;
                        full_pool |= reaches_limit;
                        self.cursor += 1;
                        continue;
                    }
                    let top = scratch.index() + values.len().checked_sub(1)?;
                    let HirExpr::Binary(mut expression) = self.scalar_value()? else {
                        return None;
                    };
                    if !values.last()?.is_computed() && !values.last()?.is_loaded_number() {
                        return None;
                    }
                    match (binary.lhs, binary.rhs) {
                        (ValueOperand::Reg(left), ValueOperand::Reg(right))
                            if values.len() >= 2
                                && left.index() + 1 == top
                                && right.index() == top
                                && binary.dst == left
                                && values[values.len() - 2].is_computed() =>
                        {
                            expression.rhs = values.pop()?.expr;
                            expression.lhs = values.pop()?.expr;
                        }
                        (ValueOperand::Reg(reg), ValueOperand::Const(k))
                        | (ValueOperand::Const(k), ValueOperand::Reg(reg))
                            if reg.index() == top
                                && binary.dst == reg
                                && values.last()?.is_computed() =>
                        {
                            if !is_finite_number(&expr_for_const(self.lowering.proto, k)) {
                                return None;
                            }
                            if binary.lhs == ValueOperand::Reg(reg) {
                                expression.lhs = values.pop()?.expr;
                            } else {
                                expression.rhs = values.pop()?.expr;
                            }
                        }
                        _ => return None,
                    }
                    values.push(RecordSlot::computed(HirExpr::Binary(expression)));
                    self.has_observable_producer = true;
                }
                LowInstr::SetTable(store)
                    if store.kind == SetTableKind::Normal
                        && store.base == AccessBase::Reg(seed.dst) =>
                {
                    let (key_expr, value_slot, materialize_primitive, call_key) = match store.key {
                        AccessKey::Const(key) if key.index() <= MAX_RK_INDEX => {
                            let expr = expr_for_const(self.lowering.proto, key);
                            // 键先于字段值求值。字段内部可能刚好填满常量池，不能用
                            // SETTABLE 时的池状态错误拒绝原本仍可用 RK 的数值键。
                            if is_finite_number(&expr) && field_pool_full {
                                return None;
                            }
                            (
                                expr,
                                scratch,
                                full_pool || key.index() >= MAX_RK_INDEX,
                                false,
                            )
                        }
                        AccessKey::Reg(key)
                            if key == scratch && values.first()?.is_materialized_key() =>
                        {
                            let key = values.remove(0);
                            let call_key = key.is_call_key();
                            (
                                key.expr,
                                Reg(scratch.index() + 1),
                                !call_key || full_pool,
                                call_key,
                            )
                        }
                        _ => return None,
                    };
                    if !call_key
                        && !matches!(key_expr, HirExpr::String(_))
                        && !is_finite_number(&key_expr)
                    {
                        return None;
                    }
                    let field_value = match (store.value, values.as_slice()) {
                        (ValueOperand::Reg(reg), [value])
                            if reg == value_slot
                                && !matches!(
                                    value.source,
                                    RecordSource::Copied | RecordSource::Argument
                                ) =>
                        {
                            values.pop()?.expr
                        }
                        (ValueOperand::Reg(reg), []) if reg.index() < self.root.index() => {
                            self.low_record_value(reg)?
                        }
                        (ValueOperand::Const(constant), []) => {
                            let expr = expr_for_const(self.lowering.proto, constant);
                            if (!matches!(
                                expr,
                                HirExpr::Nil | HirExpr::Boolean(_) | HirExpr::String(_)
                            ) && !is_finite_number(&expr))
                                || (materialize_primitive && !matches!(expr, HirExpr::String(_)))
                            {
                                return None;
                            }
                            expr
                        }
                        _ => return None,
                    };
                    fields.push(HirTableField::Record(HirRecordField {
                        key: HirTableKey::Expr(key_expr),
                        value: field_value,
                    }));
                    field_pool_full = full_pool || reaches_limit;
                }
                LowInstr::NewTable(next) if next.dst == scratch => break,
                LowInstr::SetList(parent) if parent.base.index() < seed.dst.index() => break,
                LowInstr::SetTable(_) if self.enclosing_record_store(seed.dst) => break,
                LowInstr::SetTable(store)
                    if depth == 0
                        && store.base == AccessBase::Env
                        && store.value == ValueOperand::Reg(seed.dst) =>
                {
                    break;
                }
                LowInstr::Closure(closure) if depth == 0 && closure.dst == scratch => break,
                _ => return None,
            }
            full_pool |= reaches_limit;
            self.cursor += 1;
        }
        if !values.is_empty()
            || fields.is_empty()
            || allocation != Lua51TableAllocation::from_field_counts(0, fields.len())
        {
            return None;
        }
        Some(HirExpr::TableConstructor(Box::new(HirTableConstructor {
            fields,
            trailing_multivalue: None,
        })))
    }
}
