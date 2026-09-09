//! Recover record children with a canonical scratch stack and ordered field writes.
//!
//! String keys outside Lua 5.1's RK range occupy the first scratch slot; values
//! then start one slot higher. Constant provenance distinguishes these keys from
//! runtime values, including LOADBOOL/LOADNIL once the pool exceeds RK capacity.
//! Numeric keys, calls, and nested record values remain excluded.

use std::collections::BTreeSet;

use super::*;
use crate::hir::common::{HirRecordField, HirTableAccess, HirTableKey};
use crate::transformer::{ConstRef, NewTableInstr};

const MAX_RK_INDEX: usize = 255;

struct RecordSlot {
    expr: HirExpr,
    source: RecordSource,
}

enum RecordSource {
    Computed,
    Loaded(ConstRef),
    Immediate,
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
                matches!(values, [value] if value.is_large_string())
                    && key.index() == owner.index() + 1
                    && dst.index() == key.index() + 1
            }
            AccessKey::Integer(_) => false,
        }
    }

    pub(super) fn record_table(&mut self, seed: NewTableInstr) -> Option<HirExpr> {
        let allocation = seed.lua51_allocation?;
        let scratch = Reg(seed.dst.index() + 1);
        let mut values: Vec<RecordSlot> = Vec::new();
        let mut fields = Vec::new();
        let mut keys = BTreeSet::new();
        // Only already-emitted operands prove the pool size. The final constant
        // count can include unrelated constants introduced after this constructor.
        let mut full_pool = self.lowering.proto.instrs[..self.cursor]
            .iter()
            .any(reaches_rk_limit);
        self.cursor += 1;
        loop {
            let instruction = self.instruction()?.clone();
            let reaches_limit = reaches_rk_limit(&instruction);
            match instruction {
                LowInstr::LoadConst(load) if load.dst.index() == scratch.index() + values.len() => {
                    let expr = expr_for_const(self.lowering.proto, load.value);
                    if !matches!(
                        expr,
                        HirExpr::String(_) | HirExpr::Integer(_) | HirExpr::Number(_)
                    ) || (load.value.index() <= MAX_RK_INDEX
                        && !self
                            .materialized_primitive_store(seed.dst, load.dst, &values, full_pool))
                        || (matches!(expr, HirExpr::String(_))
                            && load.value.index() <= MAX_RK_INDEX)
                    {
                        return None;
                    }
                    values.push(RecordSlot {
                        expr,
                        source: RecordSource::Loaded(load.value),
                    });
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
                    let top = scratch.index() + values.len().checked_sub(1)?;
                    let HirExpr::Binary(mut expression) = self.scalar_value()? else {
                        return None;
                    };
                    if !values.last()?.is_computed() {
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
                            if reg.index() == top && binary.dst == reg =>
                        {
                            if !matches!(
                                expr_for_const(self.lowering.proto, k),
                                HirExpr::Integer(_) | HirExpr::Number(_)
                            ) {
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
                    let (key_expr, value_slot, materialize_primitive) = match store.key {
                        AccessKey::Const(key) => (
                            expr_for_const(self.lowering.proto, key),
                            scratch,
                            full_pool || key.index() >= MAX_RK_INDEX,
                        ),
                        AccessKey::Reg(key)
                            if key == scratch && values.first()?.is_large_string() =>
                        {
                            (values.remove(0).expr, Reg(scratch.index() + 1), true)
                        }
                        _ => return None,
                    };
                    let HirExpr::String(key) = key_expr else {
                        return None;
                    };
                    if !keys.insert(key.clone()) {
                        return None;
                    }
                    let field_value = match (store.value, values.as_slice()) {
                        (ValueOperand::Reg(reg), [_]) if reg == value_slot => values.pop()?.expr,
                        (ValueOperand::Const(constant), []) => {
                            let expr = expr_for_const(self.lowering.proto, constant);
                            if !matches!(
                                expr,
                                HirExpr::Nil
                                    | HirExpr::Boolean(_)
                                    | HirExpr::Integer(_)
                                    | HirExpr::Number(_)
                                    | HirExpr::String(_)
                            ) || (materialize_primitive && !matches!(expr, HirExpr::String(_)))
                            {
                                return None;
                            }
                            expr
                        }
                        _ => return None,
                    };
                    fields.push(HirTableField::Record(HirRecordField {
                        key: HirTableKey::Expr(HirExpr::String(key)),
                        value: field_value,
                    }));
                }
                LowInstr::NewTable(next) if next.dst == scratch => break,
                LowInstr::SetList(parent) if parent.base.index() < seed.dst.index() => break,
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
