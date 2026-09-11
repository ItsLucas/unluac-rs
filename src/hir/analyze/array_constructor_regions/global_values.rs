//! 原子恢复不保留局部槽的全局赋值，供后续构造器继续证明入口帧。
//!
//! 每个标量表达式必须遵守 Lua 5.1 的临时寄存器栈，并立即 SETGLOBAL。
//! 完整替换原始 HIR 区间后才发布 closed-region 证据；不能只跳过原始
//! 指令，再假定普通 temp-inline 一定会消除这些临时局部。

use super::*;
use crate::hir::common::HirTableAccess;
use crate::transformer::{CaptureSource, ConstRef};

const MAX_RK_INDEX: usize = 255;

enum ValueSource {
    Computed,
    Loaded(ConstRef),
    Immediate,
}

struct Value {
    expression: HirExpr,
    source: ValueSource,
    pool_full_before: bool,
}

impl Value {
    fn computed(expression: HirExpr, pool_full_before: bool) -> Self {
        Self {
            expression,
            source: ValueSource::Computed,
            pool_full_before,
        }
    }

    fn is_computed(&self) -> bool {
        matches!(self.source, ValueSource::Computed)
    }

    fn is_large_string(&self) -> bool {
        matches!(self.source, ValueSource::Loaded(key) if key.index() > MAX_RK_INDEX)
            && matches!(self.expression, HirExpr::String(_))
    }

    fn is_loaded_number(&self) -> bool {
        matches!(self.source, ValueSource::Loaded(_)) && finite_number(&self.expression)
    }
}

fn finite_number(expression: &HirExpr) -> bool {
    matches!(expression, HirExpr::Integer(_))
        || matches!(expression, HirExpr::Number(value) if value.is_finite())
}

fn reaches_rk_capacity(instruction: &LowInstr) -> bool {
    let key_full = |key| matches!(key, AccessKey::Const(key) if key.index() >= MAX_RK_INDEX);
    let value_full =
        |value| matches!(value, ValueOperand::Const(value) if value.index() >= MAX_RK_INDEX);
    match instruction {
        LowInstr::LoadConst(load) => load.value.index() >= MAX_RK_INDEX,
        LowInstr::GetTable(get) => key_full(get.key),
        LowInstr::SetTable(store) => key_full(store.key) || value_full(store.value),
        LowInstr::BinaryOp(binary) => value_full(binary.lhs) || value_full(binary.rhs),
        _ => false,
    }
}

pub(super) fn global_value_region(
    stmts: &[HirStmt],
    lowering: &ProtoLowering<'_>,
    closed_arrays: &[ClosedArrayRegion],
) -> Option<(HirStmt, usize, ClosedArrayRegion)> {
    let HirStmt::Assign(seed) = stmts.first()? else {
        return None;
    };
    let ([HirLValue::Temp(root)], [_], None) = (
        seed.targets.as_slice(),
        seed.values.fixed.as_slice(),
        &seed.values.tail,
    ) else {
        return None;
    };
    let definition = lowering.dataflow.defs.get(root.index())?;
    if lowering.bindings.fixed_temps.get(definition.id.index()) != Some(root) {
        return None;
    }
    let start = definition.instr.index();
    let register = lowering.dataflow.def_reg(definition.id);
    if !matches!(
        lowering.proto.instrs.get(start)?,
        LowInstr::LoadConst(_)
            | LowInstr::LoadBool(_)
            | LowInstr::LoadNil(_)
            | LowInstr::GetTable(_)
            | LowInstr::Closure(_)
    ) || !prefix_with_closed_arrays(
        lowering.proto,
        lowering.cfg,
        lowering.dataflow,
        start,
        register,
        closed_arrays,
    ) {
        return None;
    }
    let (expression, end) = expression_region(lowering, definition.block, start, register)?;
    let mut expected = Vec::new();
    for pc in start..end {
        for def in &lowering.dataflow.instr_defs[pc] {
            let temp = TempId(def.index());
            if lowering.bindings.fixed_temps.get(def.index()) != Some(&temp)
                || lowering.bindings.lvalue_for_temp(temp) != HirLValue::Temp(temp)
                || definition_is_captured(lowering, *def)
                || lowering
                    .bindings
                    .temp_debug_locals
                    .get(temp.index())
                    .is_some_and(Option::is_some)
                || !lowering.dataflow.def_phi_uses[def.index()].is_empty()
                || lowering.dataflow.def_uses[def.index()]
                    .iter()
                    .any(|site| !(start..end).contains(&site.instr.index()))
            {
                return None;
            }
        }
        expected.extend(lower_regular_instr(
            lowering,
            definition.block,
            InstrRef(pc),
            &lowering.proto.instrs[pc],
        )?);
    }
    if stmts.get(..expected.len()) != Some(expected.as_slice()) {
        return None;
    }
    let mut replacement = expected.last()?.clone();
    let HirStmt::Assign(store) = &mut replacement else {
        return None;
    };
    store.values = vec![expression].into();
    Some((
        replacement,
        expected.len(),
        ClosedArrayRegion {
            start,
            end,
            root: register,
            retained: false,
        },
    ))
}

fn scalar_value(lowering: &ProtoLowering<'_>, block: BlockRef, pc: usize) -> Option<HirExpr> {
    let statements = lower_regular_instr(
        lowering,
        block,
        InstrRef(pc),
        lowering.proto.instrs.get(pc)?,
    )?;
    let [HirStmt::Assign(assign)] = statements.as_slice() else {
        return None;
    };
    let ([HirLValue::Temp(_)], [expression], None) = (
        assign.targets.as_slice(),
        assign.values.fixed.as_slice(),
        &assign.values.tail,
    ) else {
        return None;
    };
    Some(expression.clone())
}

fn expression_region(
    lowering: &ProtoLowering<'_>,
    block: BlockRef,
    start: usize,
    root: Reg,
) -> Option<(HirExpr, usize)> {
    let mut values: Vec<Value> = Vec::new();
    let mut full_pool = lowering.proto.instrs[..start]
        .iter()
        .any(reaches_rk_capacity);
    for pc in start..start.saturating_add(MAX_REGION_INSTRS) {
        if lowering.cfg.instr_to_block.get(pc) != Some(&block) {
            return None;
        }
        let instruction = lowering.proto.instrs.get(pc)?;
        let next = root.index() + values.len();
        match instruction {
            LowInstr::LoadConst(load) if load.dst.index() == next => {
                let expression = expr_for_const(lowering.proto, load.value);
                if !matches!(expression, HirExpr::String(_)) && !finite_number(&expression) {
                    return None;
                }
                if !values.is_empty() {
                    let large_key = load.value.index() > MAX_RK_INDEX
                        && matches!(expression, HirExpr::String(_))
                        && values.last()?.is_computed();
                    // A numeric RHS is materialized only after the runtime LHS.
                    // An unread constant or a foldable constant-only expression
                    // cannot supply a source-stack certificate.
                    let numeric_rhs = finite_number(&expression)
                        && (full_pool || load.value.index() > MAX_RK_INDEX)
                        && values.last()?.is_computed()
                        && matches!(lowering.proto.instrs.get(pc + 1),
                            Some(LowInstr::BinaryOp(binary))
                                if binary.dst.index() + 1 == load.dst.index()
                                    && binary.lhs == ValueOperand::Reg(binary.dst)
                                    && binary.rhs == ValueOperand::Reg(load.dst));
                    if !large_key && !numeric_rhs {
                        return None;
                    }
                }
                values.push(Value {
                    expression,
                    source: ValueSource::Loaded(load.value),
                    pool_full_before: full_pool,
                });
            }
            LowInstr::LoadBool(load) if values.is_empty() && load.dst == root => {
                values.push(Value {
                    expression: HirExpr::Boolean(load.value),
                    source: ValueSource::Immediate,
                    pool_full_before: full_pool,
                });
            }
            LowInstr::LoadNil(load)
                if values.is_empty() && load.dst.start == root && load.dst.len == 1 =>
            {
                values.push(Value {
                    expression: HirExpr::Nil,
                    source: ValueSource::Immediate,
                    pool_full_before: full_pool,
                });
            }
            LowInstr::Closure(closure)
                if values.is_empty()
                    && closure.dst == root
                    && closure.captures.iter().all(|capture| match capture.source {
                        CaptureSource::ByReference(reg) => reg.index() < root.index(),
                        CaptureSource::Upvalue(_) => true,
                        CaptureSource::ByValue(_) => false,
                    }) =>
            {
                let expression = scalar_value(lowering, block, pc)?;
                if !matches!(expression, HirExpr::Closure(_)) {
                    return None;
                }
                // Keep the original child/capture mapping. A closure is admitted
                // only as the entire RHS, never as an arithmetic/lookup producer.
                values.push(Value {
                    expression,
                    source: ValueSource::Immediate,
                    pool_full_before: full_pool,
                });
            }
            LowInstr::GetTable(get) if get.kind == GetTableKind::Normal => match get.key {
                AccessKey::Const(key)
                    if get.dst.index() == next
                        && get.base == AccessBase::Env
                        && matches!(expr_for_const(lowering.proto, key), HirExpr::String(_)) =>
                {
                    values.push(Value::computed(
                        scalar_value(lowering, block, pc)?,
                        full_pool,
                    ));
                }
                AccessKey::Const(key)
                    if key.index() <= MAX_RK_INDEX
                        && get.dst.index() + 1 == next
                        && get.base == AccessBase::Reg(get.dst)
                        && values.last()?.is_computed() =>
                {
                    let key = expr_for_const(lowering.proto, key);
                    if !matches!(key, HirExpr::String(_)) && (!finite_number(&key) || full_pool) {
                        return None;
                    }
                    let base = values.pop()?;
                    values.push(Value::computed(
                        HirExpr::TableAccess(Box::new(HirTableAccess {
                            base: base.expression,
                            key,
                        })),
                        base.pool_full_before,
                    ));
                }
                AccessKey::Reg(key)
                    if key.index() + 1 == next
                        && get.dst.index() + 1 == key.index()
                        && get.base == AccessBase::Reg(get.dst)
                        && values.last()?.is_large_string() =>
                {
                    let key = values.pop()?.expression;
                    if !values.last()?.is_computed() {
                        return None;
                    }
                    let base = values.pop()?;
                    values.push(Value::computed(
                        HirExpr::TableAccess(Box::new(HirTableAccess {
                            base: base.expression,
                            key,
                        })),
                        base.pool_full_before,
                    ));
                }
                _ => return None,
            },
            LowInstr::BinaryOp(binary) => {
                let top = next.checked_sub(1)?;
                let HirExpr::Binary(mut expression) = scalar_value(lowering, block, pc)? else {
                    return None;
                };
                if !matches!(
                    expression.op,
                    HirBinaryOpKind::Add
                        | HirBinaryOpKind::Sub
                        | HirBinaryOpKind::Mul
                        | HirBinaryOpKind::Div
                        | HirBinaryOpKind::Mod
                        | HirBinaryOpKind::Pow
                ) {
                    return None;
                }
                let pool_full_before = match (binary.lhs, binary.rhs) {
                    (ValueOperand::Reg(left), ValueOperand::Reg(right))
                        if values.len() >= 2
                            && left.index() + 1 == top
                            && right.index() == top
                            && binary.dst == left
                            && values[values.len() - 2].is_computed()
                            && (values.last()?.is_computed()
                                || values.last()?.is_loaded_number()) =>
                    {
                        expression.rhs = values.pop()?.expression;
                        let lhs = values.pop()?;
                        expression.lhs = lhs.expression;
                        lhs.pool_full_before
                    }
                    (ValueOperand::Reg(reg), ValueOperand::Const(key))
                    | (ValueOperand::Const(key), ValueOperand::Reg(reg))
                        if key.index() <= MAX_RK_INDEX
                            && reg.index() == top
                            && binary.dst == reg
                            && values.last()?.is_computed()
                            && finite_number(&expr_for_const(lowering.proto, key)) =>
                    {
                        let operand = values.pop()?;
                        if binary.lhs == ValueOperand::Reg(reg) {
                            if full_pool {
                                return None;
                            }
                            expression.lhs = operand.expression;
                        } else {
                            // A numeric left operand enters RK before the right
                            // expression is evaluated. The right expression may
                            // itself introduce the constant that fills the pool.
                            if operand.pool_full_before {
                                return None;
                            }
                            expression.rhs = operand.expression;
                        }
                        operand.pool_full_before
                    }
                    _ => return None,
                };
                values.push(Value::computed(
                    HirExpr::Binary(expression),
                    pool_full_before,
                ));
            }
            LowInstr::SetTable(store)
                if store.kind == SetTableKind::Normal
                    && store.base == AccessBase::Env
                    && matches!(store.key, AccessKey::Const(key)
                        if matches!(expr_for_const(lowering.proto, key), HirExpr::String(_)))
                    && store.value == ValueOperand::Reg(root)
                    && values.len() == 1 =>
            {
                return Some((values.pop()?.expression, pc + 1));
            }
            _ => return None,
        }
        full_pool |= reaches_rk_capacity(instruction);
    }
    None
}
