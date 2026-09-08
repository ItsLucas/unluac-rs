//! 开放构造器的固定前缀可在物理槽随即退役时原子收回。
//!
//! 依赖 HIR 已保存的单值 call、真正 open tail 和 trusted home；不猜调用返回类型，
//! 不删除后续覆盖。`local a=f(); local b=g(); SETLIST t(a,b,h()); a={}; b=0`
//! 只有在 fresh t 始终私有、a/b 无 debug/capture 且紧邻覆盖先于任何 t 使用时，才成为
//! `local t={f(),g(),h()}; local a={}; local b=0`。对象在覆盖前仍由 t 强引用，
//! producer 的两个旧物化身份与其下一次声明必须在同一事务中交接。

use crate::ast::DecompileDialect;
use crate::hir::common::{
    HirBlock, HirExpr, HirLValue, HirLocalDecl, HirStmt, HirTableConstructor, HirTableField,
};
use crate::hir::promotion::HomeSlotKey;

use super::bindings::{binding_from_expr, expr_uses_binding};
use super::scan::{constructor_seed, install_constructor_seed};
use super::{TableBinding, TableConstructorPass, expr_is_open_tail_safe};

pub(super) fn rebuild_retired_prefixes(
    pass: &TableConstructorPass<'_>,
    block: &mut HirBlock,
) -> bool {
    if pass.dialect != DecompileDialect::Lua51 || pass.promotion_facts.compacts_home_slots() {
        return false;
    }
    let mut changed = false;
    let mut seed_index = 0;
    while seed_index < block.stmts.len() {
        let Some((constructor, batch_index, count)) = retired_prefix(pass, block, seed_index)
        else {
            seed_index += 1;
            continue;
        };
        install_constructor_seed(&mut block.stmts[seed_index], constructor);
        for stmt in &mut block.stmts[batch_index + 1..=batch_index + count] {
            let HirStmt::Assign(assign) = stmt else {
                unreachable!("retired producer has a proved adjacent assignment");
            };
            let HirLValue::Local(local) = assign.targets[0] else {
                unreachable!("retired producer is a local binding");
            };
            *stmt = HirStmt::LocalDecl(Box::new(HirLocalDecl {
                bindings: vec![local],
                values: assign.values.clone(),
            }));
        }
        block.stmts.drain(seed_index + 1..=batch_index);
        changed = true;
        seed_index += 1;
    }
    changed
}

fn retired_prefix(
    pass: &TableConstructorPass<'_>,
    block: &HirBlock,
    seed_index: usize,
) -> Option<(HirTableConstructor, usize, usize)> {
    let (owner, seed) = constructor_seed(&block.stmts[seed_index])?;
    let TableBinding::Local(local) = owner else {
        return None;
    };
    let owner_home = private_local_home(pass, owner)?;
    if !matches!(&block.stmts[seed_index], HirStmt::LocalDecl(_))
        || !seed.fields.is_empty()
        || seed.trailing_multivalue.is_some()
        || !pass.promotion_facts.is_direct_table_seed_local(local)
        || pass.binding_is_shared_before_seed(block, seed_index, owner)
    {
        return None;
    }
    let mut producers = Vec::new();
    let mut values = Vec::new();
    for index in seed_index + 1..block.stmts.len() {
        match &block.stmts[index] {
            HirStmt::LocalDecl(decl) => {
                let [local] = decl.bindings.as_slice() else {
                    return None;
                };
                let [value @ HirExpr::Call(_)] = decl.values.fixed.as_slice() else {
                    return None;
                };
                let producer = TableBinding::Local(*local);
                let home = private_local_home(pass, producer)?;
                if decl.values.tail.is_some()
                    || home.offset_from(owner_home) != Some(producers.len() + 1)
                    || producers.iter().any(|(_, prior_home)| *prior_home == home)
                    || pass.binding_is_shared_before_seed(block, index, producer)
                    || !expr_is_open_tail_safe(value)
                    || expr_uses_binding(value, owner)
                    || expr_uses_binding(value, producer)
                    || producers
                        .iter()
                        .any(|(binding, _)| expr_uses_binding(value, *binding))
                {
                    return None;
                }
                producers.push((producer, home));
                values.push(value.clone());
            }
            HirStmt::TableSetList(batch) => {
                let tail = batch.values.tail.as_ref()?;
                if producers.is_empty()
                    || binding_from_expr(&batch.base) != Some(owner)
                    || batch.start_index != 1
                    || tail.exact_width().is_some()
                    || !expr_is_open_tail_safe(tail.as_expr())
                    || expr_uses_binding(tail.as_expr(), owner)
                    || batch.values.fixed.len() != producers.len()
                    || batch
                        .values
                        .fixed
                        .iter()
                        .zip(&producers)
                        .any(|(value, (binding, _))| {
                            binding_from_expr(value) != Some(*binding)
                                || expr_uses_binding(tail.as_expr(), *binding)
                        })
                {
                    return None;
                }
                let overwrites = block.stmts.get(index + 1..=index + producers.len())?;
                for (stmt, (binding, _)) in overwrites.iter().zip(&producers) {
                    let HirStmt::Assign(assign) = stmt else {
                        return None;
                    };
                    let TableBinding::Local(local) = binding else {
                        return None;
                    };
                    if assign.targets.as_slice() != [HirLValue::Local(*local)]
                        || assign.values.tail.is_some()
                        || assign.values.fixed.len() != 1
                        || assign.values.fixed.iter().any(|value| {
                            !expr_is_open_tail_safe(value)
                                || expr_uses_binding(value, owner)
                                || producers
                                    .iter()
                                    .any(|(producer, _)| expr_uses_binding(value, *producer))
                        })
                    {
                        return None;
                    }
                }
                return Some((
                    HirTableConstructor {
                        fields: values.into_iter().map(HirTableField::Array).collect(),
                        trailing_multivalue: Some(tail.clone()),
                    },
                    index,
                    producers.len(),
                ));
            }
            _ => return None,
        }
    }
    None
}

fn private_local_home(
    pass: &TableConstructorPass<'_>,
    binding: TableBinding,
) -> Option<HomeSlotKey> {
    let TableBinding::Local(local) = binding else {
        return None;
    };
    let home = pass.promotion_facts.trusted_local_home_slot(local)?;
    (!pass
        .debug_identity_bindings
        .get(binding)
        .copied()
        .unwrap_or_default()
        && !pass
            .reference_captured_bindings
            .get(binding)
            .copied()
            .unwrap_or_default()
        && !pass.reference_captured_home_slots.contains(&home))
    .then_some(home)
}
