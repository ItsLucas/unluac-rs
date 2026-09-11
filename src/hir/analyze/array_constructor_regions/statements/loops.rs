//! 将冻结的 Lua 5.1 generic-for 协议恢复为完整源码帧内的一条循环语句。
//!
//! 依赖 StructurePlan 与原始 iterator/state/control、TFORLOOP、跳转完全一致。
//! 例如 `for key, value in pairs(rows) do ... end` 保留三个隐藏槽和两个 pinned
//! 可见绑定；每次迭代与退出时的旧值及写入由原协议重现。隐藏槽不暴露为 Lua local，
//! 协议异常、外部读取隐藏槽及无法完整证明的循环体均拒绝。

use super::*;
use crate::hir::common::{HirGenericFor, HirPackTail, HirValuePack};
use crate::structure::LoopVmProtocol;

impl StatementParser<'_, '_> {
    pub(super) fn generic_for(
        &mut self,
        pc: &mut usize,
        end: usize,
        active: &Frame,
        call: HirCallExpr,
        depth: usize,
    ) -> Option<HirStmt> {
        let LowInstr::Jump(jump) = *self.lowering.proto.instrs.get(*pc)? else {
            return None;
        };
        let latch = jump.target.index();
        if latch <= *pc || latch + 2 > end {
            return None;
        }
        let base = active.len();
        let LowInstr::GenericForCall(next) = *self.lowering.proto.instrs.get(latch)? else {
            return None;
        };
        let LowInstr::GenericForLoop(step) = *self.lowering.proto.instrs.get(latch + 1)? else {
            return None;
        };
        let ResultPack::Fixed(results) = next.results else {
            return None;
        };
        if next.iterator.index() != base
            || next.state.index() != base + 1
            || next.control.index() != base + 2
            || step.control_target != next.control
            || step.bindings != results
            || results.start.index() != base + 3
            || results.len == 0
            || step.body_target.index() != *pc + 1
            || step.exit_target.index() != latch + 2
            || base + 3 + results.len > crate::SOURCE_LOCAL_LIMIT
        {
            return None;
        }
        let plan = self.lowering.structure.plan();
        let mut loops = plan.loops().filter(|(id, _)| {
            matches!(plan.loop_protocol(*id),
            Some(LoopVmProtocol::GenericFor(protocol))
                if protocol.call_instr.index() == latch && protocol.loop_instr.index() == latch + 1
                    && protocol.prep_instr.is_none() && !protocol.immediate_break
                    && protocol.iterator.start.index() == base && protocol.iterator.len == 3
                    && protocol.bindings == results)
        });
        let (loop_id, loop_plan) = loops.next()?;
        if loops.next().is_some() {
            return None;
        }
        let original_bindings = self
            .lowering
            .bindings
            .generic_for_locals
            .get(&loop_plan.header)?;
        if original_bindings.len() != results.len {
            return None;
        }
        let mut loop_frame = active.clone();
        for _ in 0..3 {
            loop_frame.push(FrameSlot {
                original: HirExpr::Nil,
                value: None,
            });
        }
        let mut bindings = Vec::new();
        for (offset, original) in original_bindings.iter().copied().enumerate() {
            let id = LocalId(self.lowering.bindings.locals.len() + self.locals.len());
            self.locals.push(FrameLocal {
                id,
                slot: base + 3 + offset,
                hint: self
                    .lowering
                    .bindings
                    .local_debug_hints
                    .get(original.index())
                    .cloned()
                    .flatten(),
            });
            loop_frame.push(FrameSlot {
                original: HirExpr::LocalRef(original),
                value: Some(HirExpr::LocalRef(id)),
            });
            bindings.push(id);
        }
        let outer_exit = self.loop_exit.replace(breaks::LoopExit {
            owner: loop_id,
            target: latch + 2,
        });
        let body = self.block(*pc + 1, latch, &mut loop_frame, depth + 1);
        self.loop_exit = outer_exit;
        let body = body?;
        *pc = latch + 2;
        Some(HirStmt::GenericFor(Box::new(HirGenericFor {
            bindings,
            iterator: HirValuePack::expanding(
                Vec::new(),
                HirPackTail::open(HirExpr::Call(Box::new(call))),
            ),
            body,
        })))
    }
}
