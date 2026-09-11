//! 把固定宽度 CALL 结果恢复为一组占据原槽位的局部声明。
//!
//! 输入调用表达式已由表达式 owner 证明，本模块继续校验原 CALL 结果范围及每个
//! SSA 定义，并依赖完整后缀的源码帧证明。例如 `local unused, owner = get()`
//! 必须保留两个绑定：删除未读结果会改变 CALL 宽度与后续弱引用寿命。
//! 只接受当前空闲槽开始的多固定结果；开放结果和原生迭代器由各自 owner 处理。

use super::*;
use crate::hir::common::{HirPackTail, HirValuePack};

impl StatementParser<'_, '_> {
    pub(super) fn fixed_results(
        &mut self,
        definition: usize,
        call: HirCallExpr,
        width: usize,
        active: &mut Frame,
    ) -> Option<HirStmt> {
        self.initializer_constants_closed(&HirExpr::Call(Box::new(call.clone())))?;
        if width < 2 || active.len().checked_add(width)? > crate::SOURCE_LOCAL_LIMIT {
            return None;
        }
        let LowInstr::Call(original) = self.lowering.proto.instrs.get(definition)? else {
            return None;
        };
        let ResultPack::Fixed(results) = original.results else {
            return None;
        };
        if original.kind != CallKind::Normal
            || original.method_name.is_some()
            || original.callee.index() != active.len()
            || results.start != original.callee
            || results.len != width
            || self.lowering.dataflow.instr_defs.get(definition)?.len() != width
        {
            return None;
        }
        let mut bindings = Vec::with_capacity(width);
        for offset in 0..width {
            let reg = Reg(results.start.index() + offset);
            let def = self
                .lowering
                .dataflow
                .instr_def_for_reg(InstrRef(definition), reg)?;
            let temp = *self.lowering.bindings.fixed_temps.get(def.index())?;
            let original = self.lowering.bindings.expr_for_temp(temp);
            let id = LocalId(self.lowering.bindings.locals.len() + self.locals.len());
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
            values: HirValuePack::expanding(
                Vec::new(),
                HirPackTail::exact(HirExpr::Call(Box::new(call)), width),
            ),
        })))
    }
}
