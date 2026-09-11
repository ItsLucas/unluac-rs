//! 在完整源码帧事务中恢复数值 for，保持循环体与退出时的寄存器布局。
//!
//! 本模块依赖已冻结的 NumericForProtocol、原 init/latch 字段及既有循环绑定映射。
//! 三个控制槽只保留位置且不可被源码表达式读取；可见绑定分配固定槽局部，循环体
//! 仍须由统一 block owner 完整恢复。例如 `for i=1,n do use(i) end` 保留三个隐藏
//! 槽和 i 的第四槽；未知跳转、控制槽读取或未闭合循环体不会获得帧证书。

use super::*;
use crate::hir::common::HirNumericFor;
use crate::structure::LoopVmProtocol;

impl StatementParser<'_, '_> {
    pub(super) fn numeric_for(
        &mut self,
        pc: &mut usize,
        end: usize,
        active: &Frame,
        header: [HirExpr; 3],
        depth: usize,
    ) -> Option<HirStmt> {
        let LowInstr::NumericForInit(init) = *self.lowering.proto.instrs.get(*pc)? else {
            return None;
        };
        let base = active.len();
        let latch = init.exit_target.index().checked_sub(1)?;
        if latch <= *pc || latch + 1 > end || base + 4 > crate::SOURCE_LOCAL_LIMIT {
            return None;
        }
        let LowInstr::NumericForLoop(step) = *self.lowering.proto.instrs.get(latch)? else {
            return None;
        };
        if init.index.index() != base
            || init.limit.index() != base + 1
            || init.step.index() != base + 2
            || init.binding.index() != base + 3
            || init.body_target.index() != *pc + 1
            || step.index != init.index
            || step.limit != init.limit
            || step.step != init.step
            || step.binding != init.binding
            || step.body_target != init.body_target
            || step.exit_target != init.exit_target
        {
            return None;
        }
        let plan = self.lowering.structure.plan();
        let mut loops = plan.loops().filter(|(id, _)| {
            matches!(plan.loop_protocol(*id), Some(LoopVmProtocol::NumericFor(protocol))
                if protocol.init_instr.index() == *pc
                    && protocol.index == init.index && protocol.limit == init.limit
                    && protocol.step == init.step && protocol.binding == init.binding)
        });
        let (loop_id, loop_plan) = loops.next()?;
        if loops.next().is_some() {
            return None;
        }
        let original = *self
            .lowering
            .bindings
            .numeric_for_locals
            .get(&loop_plan.header)?;
        let mut loop_frame = active.clone();
        for _ in 0..3 {
            loop_frame.push(FrameSlot {
                original: HirExpr::Nil,
                value: None,
            });
        }
        let binding = LocalId(self.lowering.bindings.locals.len() + self.locals.len());
        self.locals.push(FrameLocal {
            id: binding,
            slot: base + 3,
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
            value: Some(HirExpr::LocalRef(binding)),
        });
        let outer_exit = self.loop_exit.replace(breaks::LoopExit {
            owner: loop_id,
            target: latch + 1,
        });
        let body = self.block(*pc + 1, latch, &mut loop_frame, depth + 1);
        self.loop_exit = outer_exit;
        let body = body?;
        *pc = latch + 1;
        let [start, limit, step] = header;
        Some(HirStmt::NumericFor(Box::new(HirNumericFor {
            binding,
            start,
            limit,
            step,
            body,
        })))
    }
}
