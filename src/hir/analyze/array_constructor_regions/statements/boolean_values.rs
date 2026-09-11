//! 恢复比较表达式的布尔物化 diamond，保留原 LOADBOOL 的目标槽。
//!
//! 依赖唯一比较消费、标准 false/跳过/true 指令序列及完整源码帧后缀证明。例如
//! `local ready = current() == Kind.ACTIVE` 必须保留比较前两个临时槽与最终 ready
//! 所在槽；只读条件不能替代物化写。仅接受 Lua 5.1 的标准 false-first 形状，
//! 非比较分支、额外入口、不同目标槽及未知局部身份均拒绝。
//! 若 free 槽布尔值紧接全局或低槽索引写回，先尝试完整 Assigned：父目标 key 在
//! RHS 前保留，参与比较顺序证明；该候选失败后再尝试不带父 reservation 的局部值。

use super::*;
use crate::hir::common::HirAssign;

impl StatementParser<'_, '_> {
    pub(super) fn materialized_boolean_statement(
        &mut self,
        pc: &mut usize,
        end: usize,
        active: &mut Frame,
    ) -> Option<HirStmt> {
        let mut next = *pc;
        let statement = match self.materialized_boolean(&mut next, end, active, None, false)? {
            ExpressionEnd::Assigned(assign) => HirStmt::Assign(Box::new(assign)),
            ExpressionEnd::Value { value, definition } => {
                self.expression_origin = Some((*pc, next));
                self.declaration(definition, value, active)?
            }
            _ => return None,
        };
        *pc = next;
        Some(statement)
    }

    pub(super) fn materialized_boolean(
        &self,
        pc: &mut usize,
        end: usize,
        active: &Frame,
        temporary: Option<&[HirExpr]>,
        consume_assignment: bool,
    ) -> Option<ExpressionEnd> {
        let branch_pc = *pc;
        let false_pc = branch_pc.checked_add(1)?;
        let jump_pc = branch_pc.checked_add(2)?;
        let true_pc = branch_pc.checked_add(3)?;
        let after = branch_pc.checked_add(4)?;
        if after > end {
            return None;
        }
        if temporary.is_some_and(|values| {
            values.is_empty() || !values.iter().all(expressions::is_materialized_expression)
        }) {
            return None;
        }
        let LowInstr::Branch(branch) = *self.lowering.proto.instrs.get(branch_pc)? else {
            return None;
        };
        if !matches!(branch.cond.subject, BranchSubject::Compare { .. }) {
            return None;
        }
        let LowInstr::LoadBool(no) = *self.lowering.proto.instrs.get(false_pc)? else {
            return None;
        };
        let LowInstr::Jump(skip) = *self.lowering.proto.instrs.get(jump_pc)? else {
            return None;
        };
        let LowInstr::LoadBool(yes) = *self.lowering.proto.instrs.get(true_pc)? else {
            return None;
        };
        if no.value || !yes.value || no.dst != yes.dst || skip.target.index() != after {
            return None;
        }
        let invert = if branch.then_target.index() == true_pc
            && branch.else_target.index() == false_pc
        {
            false
        } else if branch.then_target.index() == false_pc && branch.else_target.index() == true_pc {
            true
        } else {
            return None;
        };
        // LOADBOOL's skip is lowered into two instructions from one raw opcode.
        // Require that origin witness, and reject other controls entering either
        // arm: an ordinary source comparison cannot reproduce those entrances.
        if self.lowering.proto.lowering_map.low_to_raw[false_pc]
            != self.lowering.proto.lowering_map.low_to_raw[jump_pc]
        {
            return None;
        }
        for (index, instruction) in self.lowering.proto.instrs.iter().enumerate() {
            if (branch_pc..after).contains(&index) {
                continue;
            }
            let enters = |target: InstrRef| (false_pc..after).contains(&target.index());
            if match instruction {
                LowInstr::Branch(branch) => {
                    enters(branch.then_target) || enters(branch.else_target)
                }
                LowInstr::Jump(jump) => enters(jump.target),
                LowInstr::NumericForInit(loop_) => {
                    enters(loop_.body_target) || enters(loop_.exit_target)
                }
                LowInstr::NumericForLoop(loop_) => {
                    enters(loop_.body_target) || enters(loop_.exit_target)
                }
                LowInstr::GenericForLoop(loop_) => {
                    enters(loop_.body_target) || enters(loop_.exit_target)
                }
                _ => false,
            } {
                return None;
            }
        }
        if consume_assignment
            && no.dst.index() == active.len()
            && after < end
            && let Some(LowInstr::SetTable(store)) = self.lowering.proto.instrs.get(after)
            && store.kind == SetTableKind::Normal
            && store.value == ValueOperand::Reg(no.dst)
            && (store.base == AccessBase::Env
                || matches!(store.base, AccessBase::Reg(owner) if owner.index()<active.len()))
        {
            let reserved_key = match store.key {
                AccessKey::Const(key) => Some(key),
                _ => None,
            };
            if let Some(condition) =
                self.condition(branch_pc, branch.cond, active, temporary, reserved_key)
            {
                let value = if invert { negate(condition) } else { condition };
                let start = if temporary.is_some() {
                    self.expression_origin?.0
                } else {
                    branch_pc
                };
                if let Some(assign) = self.direct_store(after, active, &mut vec![value], *store)
                    && self.assignment_constant_order(start, after + 1, &assign)
                {
                    *pc = after + 1;
                    return Some(ExpressionEnd::Assigned(assign));
                }
            }
        }
        let condition = self.condition(branch_pc, branch.cond, active, temporary, None)?;
        let value = if invert { negate(condition) } else { condition };
        let result = if no.dst.index() == active.len() {
            ExpressionEnd::Value {
                value,
                definition: true_pc,
            }
        } else if no.dst.index() < active.len() {
            let HirExpr::LocalRef(local) = active[no.dst.index()].value.as_ref()? else {
                return None;
            };
            ExpressionEnd::Assigned(HirAssign {
                targets: vec![HirLValue::Local(*local)],
                values: vec![value].into(),
            })
        } else {
            return None;
        };
        *pc = after;
        Some(result)
    }
}
