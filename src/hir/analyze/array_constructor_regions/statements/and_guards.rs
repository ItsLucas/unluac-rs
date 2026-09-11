//! 把共享 false 后继的条件序列恢复为短路 AND，保留 then/else 的原始顺序。
//!
//! 依赖原始分支的唯一前向落空边、逐段认证的临时表达式栈和完整正文帧证明。
//! 例如 `if kind == 1 and check() then A() elseif kind == 2 then B() end`
//! 只有首条件成功才调用 check，任一条件失败都进入同一 elseif。不能把该形状
//! 分成两个嵌套 if，否则第二条件失败时会漏掉 else；未知跳转或栈消费仍拒绝。

use super::*;
use crate::hir::common::HirLogicalExpr;

impl StatementParser<'_, '_> {
    pub(super) fn and_branch(
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
        let shared_end = self.arm_target(shared, end)?;
        if shared_end <= pc + 1 {
            return None;
        }
        let first_condition = self.condition(pc, first.cond, active, temporary, None)?;
        let mut conditions = vec![if first.then_target.index() == pc + 1 {
            first_condition
        } else {
            negate(first_condition)
        }];
        let mut body_start = pc + 1;
        for _ in 0..MAX_DEPTH {
            let mut candidate = self.clone();
            let mut cursor = body_start;
            let values = if matches!(
                self.lowering.proto.instrs.get(cursor),
                Some(LowInstr::Branch(_))
            ) {
                None
            } else {
                match candidate.expression(&mut cursor, shared_end, active) {
                    Some(ExpressionEnd::Value { value, .. })
                        if expressions::is_materialized_expression(&value) =>
                    {
                        Some(vec![value])
                    }
                    Some(ExpressionEnd::ConditionValues(values)) => Some(values),
                    _ => break,
                }
            };
            let Some(LowInstr::Branch(branch)) = self.lowering.proto.instrs.get(cursor).cloned()
            else {
                break;
            };
            let invert = if branch.then_target.index() == cursor + 1
                && branch.else_target.index() == shared
            {
                false
            } else if branch.else_target.index() == cursor + 1
                && branch.then_target.index() == shared
            {
                true
            } else {
                break;
            };
            if cursor + 1 >= shared_end {
                break;
            }
            let condition =
                candidate.condition(cursor, branch.cond, active, values.as_deref(), None)?;
            conditions.push(if invert { negate(condition) } else { condition });
            body_start = cursor + 1;
            *self = candidate;
        }
        if conditions.len() < 2 {
            return None;
        }
        let (body_end, merge, arm_exit) = match self.lowering.proto.instrs.get(shared_end - 1)? {
            LowInstr::Jump(jump)
                if jump.target.index() >= shared_end && !self.is_loop_break(shared_end - 1) =>
            {
                (
                    shared_end - 1,
                    self.arm_target(jump.target.index(), end)?,
                    Some(jump.target.index()),
                )
            }
            _ => (shared_end, shared_end, None),
        };
        if body_end < body_start {
            return None;
        }
        let body = self.arm_block(
            body_start,
            body_end,
            &mut active.clone(),
            depth + 1,
            arm_exit,
        )?;
        let else_block = if merge > shared_end {
            Some(self.block(shared_end, merge, &mut active.clone(), depth + 1)?)
        } else {
            None
        };
        let mut conditions = conditions.into_iter();
        let first = conditions.next()?;
        let condition = conditions.fold(first, |lhs, rhs| {
            HirExpr::LogicalAnd(Box::new(HirLogicalExpr { lhs, rhs }))
        });
        Some((
            HirStmt::If(Box::new(HirIf {
                cond: condition,
                then_block: body,
                else_block,
            })),
            merge,
        ))
    }
}
