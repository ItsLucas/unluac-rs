//! 完整源码帧中的 break 只消费当前词法循环已冻结的退出边。
//!
//! 例如 generic-for 正文末尾的 `JMP exit` 恢复为 `break`，依赖外层已匹配的
//! LoopPlan、精确 CFG 边和 EdgeTransfer::Break；不能把任意前向跳转当作退出。
//! 需要额外 cleanup、转发路线或迭代槽处置的边继续拒绝，原始槽写由整帧事务验证。

use super::*;
use crate::structure::{EdgeTransfer, LoopPlanId};

#[derive(Clone, Copy)]
pub(super) struct LoopExit {
    pub(super) owner: LoopPlanId,
    pub(super) target: usize,
}

impl StatementParser<'_, '_> {
    pub(super) fn is_loop_break(&self, pc: usize) -> bool {
        self.prove_loop_break(pc).is_some()
    }

    fn prove_loop_break(&self, pc: usize) -> Option<()> {
        let LoopExit { owner, target } = self.loop_exit?;
        let LowInstr::Jump(jump) = self.lowering.proto.instrs.get(pc)? else {
            return None;
        };
        if jump.target.index() != target {
            return None;
        }
        let cfg = self.lowering.cfg;
        let block = *cfg.instr_to_block.get(pc)?;
        let source = cfg.blocks.get(block.index())?;
        if source.instrs.start.index() + source.instrs.len != pc + 1 {
            return None;
        }
        let [edge] = cfg.succs.get(block.index())?.as_slice() else {
            return None;
        };
        let target_block = *cfg.instr_to_block.get(target)?;
        if cfg.edges.get(edge.index())?.to != target_block {
            return None;
        }
        let plan = self.lowering.structure.plan();
        let loop_plan = plan.loop_(owner)?;
        let edge_plan = plan.edge_plan(*edge)?;
        if !loop_plan.break_edges.contains(edge)
            || edge_plan.transfer != EdgeTransfer::Break(plan.loop_region(owner)?)
            || edge_plan.forward_route.is_some()
            || edge_plan.actions_before_trailing_cleanup().is_some()
            || !edge_plan.iteration.is_empty()
        {
            return None;
        }
        Some(())
    }
}
