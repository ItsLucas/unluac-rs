//! raw SSA producer 的物理覆盖证明，独立于 RHS 是否为标量。
//!
//! `t = 5` 仍可能清掉同槽旧 call result；单定义 temp 并不证明这个覆盖可删。
//! 这里只在线性 block 内记住已知 primitive 槽，根级单次执行时另接受入口 nil；
//! 隐藏 scratch 写入、未知 home 和控制流均清空事实，不猜对象是否仍被其它引用保活。
//! 例如 `R3 = f(); R3 = 5` 保留覆盖，`R3 = 4; R3 = 5` 才可进入完整构造器事务。

use std::collections::BTreeSet;

use crate::hir::common::{HirExpr, HirLValue, HirStmt, HirValuePack, TempId};
use crate::hir::promotion::{HomeSlotKey, ProtoPromotionFacts};

use super::{expr_is_data_only, producer_value_can_be_dropped};

#[derive(Clone, Copy)]
pub(super) struct RawProducerPolicy<'a> {
    pub(super) primitive: bool,
    pub(super) entry_allocations: bool,
    pub(super) promotion_facts: &'a ProtoPromotionFacts,
    pub(super) safe_overwrites: &'a BTreeSet<TempId>,
}

pub(super) fn safe_producer_overwrites(
    stmts: &[HirStmt],
    facts: &ProtoPromotionFacts,
    single_pass_root: bool,
) -> BTreeSet<TempId> {
    let mut primitive_homes = BTreeSet::<HomeSlotKey>::new();
    let mut safe = BTreeSet::new();
    for stmt in stmts {
        let (targets, values) = match stmt {
            HirStmt::Assign(assign)
                if assign
                    .targets
                    .iter()
                    .all(|target| matches!(target, HirLValue::Temp(_) | HirLValue::Local(_))) =>
            {
                (assign.targets.clone(), &assign.values)
            }
            HirStmt::LocalDecl(decl) => (
                decl.bindings
                    .iter()
                    .copied()
                    .map(HirLValue::Local)
                    .collect(),
                &decl.values,
            ),
            HirStmt::Assign(assign)
                if assign.targets.iter().all(|target| {
                    matches!(target, HirLValue::TableAccess(access)
                        if plain_value(&access.base) && plain_value(&access.key))
                }) && plain_pack(&assign.values) =>
            {
                continue;
            }
            HirStmt::TableSetList(batch)
                if plain_value(&batch.base)
                    && batch.values.tail.is_none()
                    && batch
                        .values
                        .fixed
                        .iter()
                        .all(|value| primitive_value(value, facts, &primitive_homes)) =>
            {
                continue;
            }
            _ => {
                primitive_homes.clear();
                continue;
            }
        };
        if !direct_pack(values) || targets.len() != values.fixed.len() {
            primitive_homes.clear();
        }
        for (index, target) in targets.iter().enumerate() {
            let home = match target {
                HirLValue::Temp(temp) => facts.trusted_temp_home_slot(*temp),
                HirLValue::Local(local) => facts.trusted_local_home_slot(*local),
                _ => None,
            };
            let Some(home) = home else {
                primitive_homes.clear();
                continue;
            };
            if let HirLValue::Temp(temp) = target
                && (primitive_homes.contains(&home)
                    || single_pass_root && facts.overwrites_entry_nil(*temp))
            {
                safe.insert(*temp);
            }
            primitive_homes.remove(&home);
            if values.fixed.get(index).is_some_and(|value| {
                producer_value_can_be_dropped(value) && expr_is_data_only(value)
            }) {
                primitive_homes.insert(home);
            }
        }
    }
    safe
}

fn plain_pack(values: &HirValuePack) -> bool {
    values.tail.is_none() && values.fixed.iter().all(plain_value)
}

fn direct_pack(values: &HirValuePack) -> bool {
    plain_pack(values)
        || values.tail.is_none()
            && matches!(values.fixed.as_slice(), [HirExpr::TableConstructor(table)]
                if table.fields.is_empty() && table.trailing_multivalue.is_none())
}

fn primitive_value(
    value: &HirExpr,
    facts: &ProtoPromotionFacts,
    primitive_homes: &BTreeSet<HomeSlotKey>,
) -> bool {
    if producer_value_can_be_dropped(value) && expr_is_data_only(value) {
        return true;
    }
    let home = match value {
        HirExpr::TempRef(temp) => facts.trusted_temp_home_slot(*temp),
        HirExpr::LocalRef(local) => facts.trusted_local_home_slot(*local),
        HirExpr::ParamRef(param) => facts.trusted_param_home_slot(*param),
        _ => None,
    };
    home.is_some_and(|home| primitive_homes.contains(&home))
}

fn plain_value(value: &HirExpr) -> bool {
    (producer_value_can_be_dropped(value) && expr_is_data_only(value))
        || matches!(
            value,
            HirExpr::ParamRef(_) | HirExpr::LocalRef(_) | HirExpr::TempRef(_)
        )
}
