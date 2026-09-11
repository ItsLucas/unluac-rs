//! 保留完整源码帧中的成组 nil 声明和紧邻全局闭包安装。
//!
//! 例如 `local unused, current; Handler = function() return current end` 必须按
//! 原 LOADNIL 范围占槽，捕获也须按原 capture 顺序指向当前 pinned local。
//! 依赖实际指令定义及低槽身份；不合并编译器会省去的 nil 写，不接受隐藏槽、
//! 自捕获或未知闭包安装出口，也不用新声明替代未证明的物理旧值。

use super::*;
use crate::hir::common::{
    HirAssign, HirCapture, HirCaptureMode, HirClosureExpr, HirGlobalRef, UpvalueId,
};
use crate::transformer::{CaptureSource, ClosureCreation};

impl StatementParser<'_, '_> {
    pub(super) fn nil_declaration(
        &mut self,
        pc: usize,
        count: usize,
        active: &mut Frame,
    ) -> Option<HirStmt> {
        if pc == 0
            || count == 0
            || active.len() + count > crate::SOURCE_LOCAL_LIMIT
            || matches!(
                self.lowering.proto.instrs.get(pc - 1),
                Some(LowInstr::LoadNil(_))
            )
        {
            return None;
        }
        let mut bindings = Vec::new();
        for _ in 0..count {
            let reg = Reg(active.len());
            let def = self
                .lowering
                .dataflow
                .instr_def_for_reg(InstrRef(pc), reg)?;
            let temp = *self.lowering.bindings.fixed_temps.get(def.index())?;
            let id = LocalId(self.lowering.bindings.locals.len() + self.locals.len());
            let original = self.lowering.bindings.expr_for_temp(temp);
            let hint = match &original {
                HirExpr::LocalRef(local) => self
                    .lowering
                    .bindings
                    .local_debug_hints
                    .get(local.index())
                    .cloned()
                    .flatten(),
                _ => self
                    .lowering
                    .bindings
                    .temp_debug_locals
                    .get(temp.index())
                    .cloned()
                    .flatten(),
            };
            self.locals.push(FrameLocal {
                id,
                slot: reg.index(),
                hint,
            });
            active.push(FrameSlot {
                original,
                value: Some(HirExpr::LocalRef(id)),
            });
            bindings.push(id);
        }
        Some(HirStmt::LocalDecl(Box::new(HirLocalDecl {
            bindings,
            values: vec![HirExpr::Nil; count].into(),
        })))
    }

    pub(super) fn closure_install(
        &self,
        pc: &mut usize,
        end: usize,
        active: &Frame,
    ) -> Option<HirStmt> {
        let LowInstr::Closure(closure) = self.lowering.proto.instrs.get(*pc)? else {
            return None;
        };
        if closure.creation != ClosureCreation::Fresh
            || closure.dst.index() != active.len()
            || *pc + 1 >= end
        {
            return None;
        }
        let LowInstr::SetTable(store) = self.lowering.proto.instrs.get(*pc + 1)? else {
            return None;
        };
        if store.kind != SetTableKind::Normal
            || store.base != AccessBase::Env
            || store.value != ValueOperand::Reg(closure.dst)
        {
            return None;
        }
        let name = crate::hir::analyze::exprs::global_name_for_access(
            self.lowering,
            self.lowering.cfg.instr_to_block[*pc + 1],
            InstrRef(*pc + 1),
            store.base,
            store.key,
        )?;
        let captures = closure
            .captures
            .iter()
            .map(|capture| {
                let value = match capture.source {
                    CaptureSource::ByReference(reg) if reg.index() < active.len() => {
                        active[reg.index()].value.clone()?
                    }
                    CaptureSource::Upvalue(upvalue) => {
                        HirExpr::UpvalueRef(UpvalueId(upvalue.index()))
                    }
                    _ => return None,
                };
                Some(HirCapture {
                    mode: HirCaptureMode::ByReference,
                    value,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let value = HirExpr::Closure(Box::new(HirClosureExpr {
            proto: *self.lowering.child_refs.get(closure.proto.index())?,
            captures,
        }));
        *pc += 2;
        Some(HirStmt::Assign(Box::new(HirAssign {
            targets: vec![HirLValue::Global(HirGlobalRef { name })],
            values: vec![value].into(),
        })))
    }
}
