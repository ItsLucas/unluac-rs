//! Recover record children with a canonical scratch stack and ordered field writes.
//!
//! String keys outside Lua 5.1's RK range occupy the first scratch slot; values
//! then start one slot higher. Constant provenance distinguishes these keys from
//! runtime values. Numeric keys, calls, and nested record values remain excluded.

use std::collections::BTreeSet;

use super::*;
use crate::hir::common::{HirRecordField, HirTableAccess, HirTableKey};
use crate::transformer::{ConstRef, NewTableInstr};

const MAX_RK_INDEX: usize = 255;

struct RecordSlot {
    expr: HirExpr,
    loaded: Option<ConstRef>,
}

impl RecordSlot {
    fn computed(expr: HirExpr) -> Self {
        Self { expr, loaded: None }
    }

    fn is_large_string(&self) -> bool {
        self.loaded.is_some_and(|key| key.index() > MAX_RK_INDEX)
            && matches!(self.expr, HirExpr::String(_))
    }
}

impl ArrayParser<'_, '_> {
    pub(super) fn record_table(&mut self, seed: NewTableInstr) -> Option<HirExpr> {
        let allocation = seed.lua51_allocation?;
        let scratch = Reg(seed.dst.index() + 1);
        let mut values: Vec<RecordSlot> = Vec::new();
        let mut fields = Vec::new();
        let mut keys = BTreeSet::new();
        self.cursor += 1;
        loop {
            match self.instruction()?.clone() {
                LowInstr::LoadConst(load) if load.dst.index() == scratch.index() + values.len() => {
                    let expr = expr_for_const(self.lowering.proto, load.value);
                    // A large string key establishes that the constant pool is
                    // already too large for even reused numeric literals to use RK.
                    let large_record_key = values.first().is_some_and(RecordSlot::is_large_string);
                    // Key 255 fits RK, but interning it makes fs->nk 256.
                    // recfield interns its key before compiling the value.
                    let threshold_record_key = values.is_empty()
                        && matches!(self.lowering.proto.instrs.get(self.cursor + 1),
                            Some(LowInstr::SetTable(store))
                                if store.base == AccessBase::Reg(seed.dst)
                                    && store.value == ValueOperand::Reg(load.dst)
                                    && matches!(store.key, AccessKey::Const(key) if key.index() == MAX_RK_INDEX));
                    if !matches!(
                        expr,
                        HirExpr::String(_) | HirExpr::Integer(_) | HirExpr::Number(_)
                    ) || (load.value.index() <= MAX_RK_INDEX
                        && !large_record_key
                        && !threshold_record_key)
                        || (matches!(expr, HirExpr::String(_))
                            && load.value.index() <= MAX_RK_INDEX)
                    {
                        return None;
                    }
                    values.push(RecordSlot {
                        expr,
                        loaded: Some(load.value),
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
                                && values.last()?.loaded.is_none()
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
                                        && values.last()?.loaded.is_none() =>
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
                    if values.last()?.loaded.is_some() {
                        return None;
                    }
                    match (binary.lhs, binary.rhs) {
                        (ValueOperand::Reg(left), ValueOperand::Reg(right))
                            if values.len() >= 2
                                && left.index() + 1 == top
                                && right.index() == top
                                && binary.dst == left
                                && values[values.len() - 2].loaded.is_none() =>
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
                    let (key_expr, value_slot) = match store.key {
                        AccessKey::Const(key) => {
                            (expr_for_const(self.lowering.proto, key), scratch)
                        }
                        AccessKey::Reg(key)
                            if key == scratch && values.first()?.is_large_string() =>
                        {
                            (values.remove(0).expr, Reg(scratch.index() + 1))
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
                            if !matches!(expr, HirExpr::Nil | HirExpr::Boolean(_) | HirExpr::Integer(_)
                                | HirExpr::Number(_) | HirExpr::String(_))
                                // With a large key, Lua 5.1 materializes numeric,
                                // boolean and nil values instead of encoding RK.
                                || (value_slot != scratch && !matches!(expr, HirExpr::String(_)))
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
