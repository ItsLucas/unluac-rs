//! 保留完整源码帧内不引入额外临时槽的全局与索引赋值。
//!
//! 例如 `center = (rows[1].x + rows[2].x) / 2` 沿原表达式栈写入 global；
//! `object.id = existing` 则直接读取已认证低槽。依赖原 SETTABLE/SETGLOBAL 的
//! owner、key、value 槽与源码帧绑定，拒绝把 MOVE 临时重写成会消除覆盖写的裸读取，
//! 不接常量池满后的 RK、隐藏槽或未匹配的 pending 临时。
//! 全局赋值还要验证名称先于 RHS 新常量入池；例如 `Global = {field = value}`
//! 不能被误选成 local 建表后再赋值，否则常量池顺序会改变。反向顺序则保留 local 解释。

use super::*;
use crate::hir::common::{HirAssign, HirGlobalRef, HirTableAccess};
use crate::transformer::SetTableInstr;

impl StatementParser<'_, '_> {
    pub(super) fn local_assignment(
        &self,
        target: Reg,
        value: HirExpr,
        active: &Frame,
    ) -> Option<HirAssign> {
        let HirExpr::LocalRef(local) = active.get(target.index())?.value.as_ref()? else {
            return None;
        };
        Some(HirAssign {
            targets: vec![HirLValue::Local(*local)],
            values: vec![value].into(),
        })
    }

    pub(super) fn assignment_constant_order(
        &self,
        start: usize,
        end: usize,
        assign: &HirAssign,
    ) -> bool {
        if !matches!(
            assign.targets.as_slice(),
            [HirLValue::Global(_) | HirLValue::TableAccess(_)]
        ) {
            return true;
        }
        let Some(LowInstr::SetTable(store)) = self.lowering.proto.instrs.get(end.saturating_sub(1))
        else {
            return false;
        };
        let AccessKey::Const(key) = store.key else {
            return matches!(store.key, AccessKey::Reg(_));
        };
        let previous = self.lowering.proto.instrs[..start]
            .iter()
            .flat_map(constant_indices)
            .max();
        let first_rhs = self.lowering.proto.instrs[start..end - 1]
            .iter()
            .flat_map(constant_indices)
            .filter(|index| previous.is_none_or(|previous| *index > previous))
            .min();
        first_rhs.is_none_or(|first_rhs| key.index() <= first_rhs)
    }

    pub(super) fn direct_store(
        &self,
        pc: usize,
        active: &Frame,
        pending: &mut Vec<HirExpr>,
        store: SetTableInstr,
    ) -> Option<HirAssign> {
        let ValueOperand::Reg(source) = store.value else {
            return None;
        };
        let value = if pending.is_empty() && source.index() < active.len() {
            active[source.index()].value.clone()?
        } else if (store.base == AccessBase::Env
            || matches!(store.base, AccessBase::Reg(owner) if owner.index()<active.len()))
            && pending.len() == 1
            && source.index() == active.len()
            && pending
                .first()
                .is_some_and(expressions::is_materialized_expression)
        {
            pending.pop()?
        } else {
            return None;
        };
        let target = match store.base {
            AccessBase::Env => HirLValue::Global(HirGlobalRef {
                name: crate::hir::analyze::exprs::global_name_for_access(
                    self.lowering,
                    self.lowering.cfg.instr_to_block[pc],
                    InstrRef(pc),
                    store.base,
                    store.key,
                )?,
            }),
            AccessBase::Reg(owner) if owner.index() < active.len() => {
                let key = match store.key {
                    AccessKey::Reg(key) if key.index() < active.len() => {
                        active[key.index()].value.clone()?
                    }
                    AccessKey::Const(key) if key.index() <= 255 => {
                        expr_for_const(self.lowering.proto, key)
                    }
                    _ => return None,
                };
                HirLValue::TableAccess(Box::new(HirTableAccess {
                    base: active[owner.index()].value.clone()?,
                    key,
                }))
            }
            _ => return None,
        };
        Some(HirAssign {
            targets: vec![target],
            values: vec![value].into(),
        })
    }
}

pub(super) fn constant_indices(instruction: &LowInstr) -> Vec<usize> {
    let mut result = Vec::new();
    let key = |key: AccessKey| match key {
        AccessKey::Const(key) => Some(key.index()),
        _ => None,
    };
    let value = |value: ValueOperand| match value {
        ValueOperand::Const(key) => Some(key.index()),
        _ => None,
    };
    match instruction {
        LowInstr::LoadConst(load) => result.push(load.value.index()),
        LowInstr::GetTable(get) => result.extend(key(get.key)),
        LowInstr::SetTable(store) => {
            result.extend(key(store.key));
            result.extend(value(store.value));
        }
        LowInstr::BinaryOp(binary) => {
            result.extend(value(binary.lhs));
            result.extend(value(binary.rhs));
        }
        LowInstr::Branch(branch) => {
            if let BranchSubject::Compare { lhs, rhs, .. } = branch.cond.subject {
                for operand in [lhs, rhs] {
                    if let CondOperand::Const(key) = operand {
                        result.push(key.index());
                    }
                }
            }
        }
        _ => {}
    }
    result
}
