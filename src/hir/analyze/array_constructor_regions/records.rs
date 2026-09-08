//! A record child uses one scratch register, released after each SETTABLE.
//!
//! Only unique string keys and primitive RK values or GETTABLE chains are accepted.
//! The whole parent array still owns the raw interval: no intermediate object local
//! survives, and a sibling NEWTABLE overwrites exactly the preceding scratch slot.
//! Numeric keys, mixed layouts, calls/open results, and nested record values stay out.

use std::collections::BTreeSet;

use super::*;
use crate::hir::common::{HirRecordField, HirTableAccess, HirTableKey};
use crate::transformer::NewTableInstr;

impl ArrayParser<'_, '_> {
    pub(super) fn record_table(&mut self, seed: NewTableInstr) -> Option<HirExpr> {
        let allocation = seed.lua51_allocation?;
        let scratch = Reg(seed.dst.index() + 1);
        let mut value = None;
        let mut fields = Vec::new();
        let mut keys = BTreeSet::new();
        self.cursor += 1;
        loop {
            match self.instruction()?.clone() {
                LowInstr::GetTable(get)
                    if get.kind == GetTableKind::Normal && get.dst == scratch =>
                {
                    let AccessKey::Const(key) = get.key else {
                        return None;
                    };
                    value = Some(match (get.base, value.take()) {
                        (AccessBase::Reg(base), Some(previous)) if base == scratch => {
                            HirExpr::TableAccess(Box::new(HirTableAccess {
                                base: previous,
                                key: expr_for_const(self.lowering.proto, key),
                            }))
                        }
                        (AccessBase::Env, None) => self.scalar_value()?,
                        (AccessBase::Reg(base), None) if base.index() < self.root.index() => {
                            self.scalar_value()?
                        }
                        _ => return None,
                    });
                    self.has_observable_producer = true;
                }
                LowInstr::SetTable(store)
                    if store.kind == SetTableKind::Normal
                        && store.base == AccessBase::Reg(seed.dst) =>
                {
                    let AccessKey::Const(key) = store.key else {
                        return None;
                    };
                    let HirExpr::String(key) = expr_for_const(self.lowering.proto, key) else {
                        return None;
                    };
                    if !keys.insert(key.clone()) {
                        return None;
                    }
                    let field_value = match (store.value, value.take()) {
                        (ValueOperand::Reg(reg), Some(value)) if reg == scratch => value,
                        (ValueOperand::Const(constant), None) => {
                            let value = expr_for_const(self.lowering.proto, constant);
                            if !matches!(
                                value,
                                HirExpr::Nil
                                    | HirExpr::Boolean(_)
                                    | HirExpr::Integer(_)
                                    | HirExpr::Number(_)
                                    | HirExpr::String(_)
                            ) {
                                return None;
                            }
                            value
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
        if value.is_some()
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
