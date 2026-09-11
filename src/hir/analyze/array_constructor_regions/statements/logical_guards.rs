//! 恢复多个前向条件共享同一后继正文的短路 OR，并保留可选 else。
//!
//! 例如 `if first() == 1 or second() == 1 then A() else B() end` 依赖每段条件
//! 原始求值栈、唯一临时消费及共享控制边；第二调用仍仅在首条件失败时执行。
//! then/else 正文不复制、不交换；不匹配的控制边、额外临时读取或会消除 NOT 写入
//! 的条件表达式均拒绝，由完整父事务继续验证后续帧。

use super::*;
use crate::hir::common::HirLogicalExpr;
use crate::transformer::BranchInstr;

impl StatementParser<'_, '_> {
    pub(super) fn or_branch(
        &mut self,
        pc: usize,
        end: usize,
        active: &Frame,
        temporary: Option<&[HirExpr]>,
        depth: usize,
    ) -> Option<(HirStmt, usize)> {
        let LowInstr::Branch(first) = *self.lowering.proto.instrs.get(pc)? else {
            return None;
        };
        let shared = if first.then_target.index() == pc + 1 {
            first.else_target.index()
        } else if first.else_target.index() == pc + 1 {
            first.then_target.index()
        } else {
            return None;
        };
        if shared <= pc + 1 || shared >= end {
            return None;
        }
        let mut conditions = vec![self.condition_to(pc, first, shared, active, temporary)?];
        let mut cursor = pc + 1;
        for _ in 0..MAX_DEPTH {
            let values = if matches!(
                self.lowering.proto.instrs.get(cursor),
                Some(LowInstr::Branch(_))
            ) {
                None
            } else {
                match self.expression(&mut cursor, shared, active)? {
                    ExpressionEnd::Value { value, .. }
                        if expressions::is_materialized_expression(&value) =>
                    {
                        Some(vec![value])
                    }
                    ExpressionEnd::ConditionValues(values) => Some(values),
                    _ => return None,
                }
            };
            let LowInstr::Branch(branch) = *self.lowering.proto.instrs.get(cursor)? else {
                return None;
            };
            let other = if branch.then_target.index() == shared {
                branch.else_target.index()
            } else if branch.else_target.index() == shared {
                branch.then_target.index()
            } else {
                return None;
            };
            conditions.push(self.condition_to(
                cursor,
                branch,
                shared,
                active,
                values.as_deref(),
            )?);
            if other == cursor + 1 && other < shared {
                cursor = other;
                continue;
            }
            let other = self.arm_target(other, end)?;
            if cursor + 1 != shared || other <= shared {
                return None;
            }
            let (body_end, merge, arm_exit) = match self.lowering.proto.instrs.get(other - 1)? {
                LowInstr::Jump(jump)
                    if jump.target.index() >= other && !self.is_loop_break(other - 1) =>
                {
                    (
                        other - 1,
                        self.arm_target(jump.target.index(), end)?,
                        Some(jump.target.index()),
                    )
                }
                _ => (other, other, None),
            };
            let body =
                self.arm_block(shared, body_end, &mut active.clone(), depth + 1, arm_exit)?;
            let else_block = if merge > other {
                Some(self.block(other, merge, &mut active.clone(), depth + 1)?)
            } else {
                None
            };
            let mut conditions = conditions.into_iter();
            let first = conditions.next()?;
            let condition = conditions.fold(first, |lhs, rhs| {
                HirExpr::LogicalOr(Box::new(HirLogicalExpr { lhs, rhs }))
            });
            return Some((
                HirStmt::If(Box::new(HirIf {
                    cond: condition,
                    then_block: body,
                    else_block,
                })),
                merge,
            ));
        }
        None
    }

    fn condition_to(
        &self,
        pc: usize,
        branch: BranchInstr,
        target: usize,
        active: &Frame,
        temporary: Option<&[HirExpr]>,
    ) -> Option<HirExpr> {
        let condition = self.condition(pc, branch.cond, active, temporary, None)?;
        if branch.then_target.index() == target {
            Some(condition)
        } else if branch.else_target.index() == target {
            Some(negate(condition))
        } else {
            None
        }
    }
}
