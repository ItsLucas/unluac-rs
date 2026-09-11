//! 复用当前分支臂已证明的正常出口，解释 Lua 编译器折叠后的跳转链。
//!
//! 例如 then 臂末尾 `JMP merge` 被内部条件直接复用时，内部目标可能在当前臂范围外。
//! 只有外层已经验证的同一个 merge 才等价于当前臂结束；不接受任意远端目标，
//! 不跨循环 break，也不跳过任何未解释指令。证书按递归词法范围保存和恢复。

use super::*;

#[derive(Clone, Copy)]
pub(super) struct ArmExit {
    end: usize,
    continuation: usize,
}

impl StatementParser<'_, '_> {
    pub(super) fn arm_target(&self, target: usize, end: usize) -> Option<usize> {
        if target <= end {
            return Some(target);
        }
        let exit = self.arm_exit?;
        (exit.end == end && exit.continuation == target).then_some(end)
    }

    pub(super) fn arm_block(
        &mut self,
        start: usize,
        end: usize,
        active: &mut Frame,
        depth: usize,
        continuation: Option<usize>,
    ) -> Option<HirBlock> {
        let outer_exit = self.arm_exit;
        if let Some(continuation) = continuation {
            if continuation <= end || self.is_loop_break(end) {
                return None;
            }
            let LowInstr::Jump(jump) = self.lowering.proto.instrs.get(end)? else {
                return None;
            };
            if jump.target.index() != continuation {
                return None;
            }
            self.arm_exit = Some(ArmExit { end, continuation });
        }
        let result = self.block(start, end, active, depth);
        self.arm_exit = outer_exit;
        result
    }
}
