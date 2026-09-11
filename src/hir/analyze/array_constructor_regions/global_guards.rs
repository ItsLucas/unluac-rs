//! 将已由 Structure 降低的单臂全局安装 guard 收为完整源码语句。
//!
//! 本模块先验证原始 `GETGLOBAL; CALL; Branch; Closure; SETGLOBAL` 的同槽协议，
//! 再精确匹配现有 HIR If 的条件和 then 语句，不重新识别或选择控制结构。
//! 例如 `t0 = IsClient(); if t0 then t1 = closure; Global = t1 end` 恢复为
//! `if IsClient() then Global = function() ... end end`，调用和闭包仍各执行一次。
//! 发布的 closed region 只覆盖这五条 LIR；后继帧与整个后缀仍由父 ledger 验证。

use super::*;
use crate::hir::common::HirIf;
use crate::transformer::{BranchSubject, CaptureSource, ClosureCreation, CondOperand};

pub(super) fn global_guard_region(
    stmts: &[HirStmt],
    lowering: &ProtoLowering<'_>,
    closed_arrays: &[ClosedArrayRegion],
) -> Option<(HirStmt, usize, ClosedArrayRegion)> {
    let HirStmt::Assign(seed) = stmts.first()? else {
        return None;
    };
    let [HirLValue::Temp(root)] = seed.targets.as_slice() else {
        return None;
    };
    let definition = lowering.dataflow.defs.get(root.index())?;
    let start = definition.instr.index();
    let end = start.checked_add(5)?;
    let [
        LowInstr::GetTable(get),
        LowInstr::Call(call),
        LowInstr::Branch(branch),
        LowInstr::Closure(closure),
        LowInstr::SetTable(store),
    ] = lowering.proto.instrs.get(start..end)?
    else {
        return None;
    };
    if get.kind != GetTableKind::Normal
        || get.base != AccessBase::Env
        || !matches!(get.key, AccessKey::Const(_))
        || call.kind != CallKind::Normal
        || call.method_name.is_some()
        || call.callee != get.dst
        || !matches!(call.args, ValuePack::Fixed(args)
            if args.start.index() == get.dst.index() + 1 && args.len == 0)
        || !matches!(call.results, ResultPack::Fixed(results)
            if results.start == get.dst && results.len == 1)
        || branch.cond.subject != BranchSubject::Truthy(CondOperand::Reg(get.dst))
        || closure.creation != ClosureCreation::Fresh
        || closure.dst != get.dst
        || !closure.captures.iter().all(|capture| match capture.source {
            CaptureSource::ByReference(reg) => reg.index() < get.dst.index(),
            CaptureSource::Upvalue(_) => true,
            CaptureSource::ByValue(_) => false,
        })
        || store.kind != SetTableKind::Normal
        || store.base != AccessBase::Env
        || !matches!(store.key, AccessKey::Const(_))
        || store.value != ValueOperand::Reg(get.dst)
        || lowering.proto.instrs.get(end).is_none()
    {
        return None;
    }
    // Lua 5.1 的 TEST/JMP 通常将 false 编码为 then_target；只接受 truthy
    // 路径恰好落入紧随的 closure，false 路径恰好越过这一次全局安装。
    let (truthy, falsy) = if branch.cond.negated {
        (branch.else_target, branch.then_target)
    } else {
        (branch.then_target, branch.else_target)
    };
    if truthy.index() != start + 3
        || falsy.index() != end
        || lowering.cfg.instr_to_block[start..start + 3]
            .iter()
            .any(|block| *block != definition.block)
        || lowering.cfg.instr_to_block[start + 3] != lowering.cfg.instr_to_block[start + 4]
        || !prefix_with_closed_arrays(
            lowering.proto,
            lowering.cfg,
            lowering.dataflow,
            start,
            get.dst,
            closed_arrays,
        )
    {
        return None;
    }

    // guard 的 callee、条件结果与 closure 都是可删除的 SSA 临时值；任何 debug
    // local、capture、phi 或后缀读取都说明源帧还需要这些绑定，不能仅靠 raw 相似收回。
    for pc in [start, start + 1, start + 3] {
        let [def] = lowering.dataflow.instr_defs[pc].as_slice() else {
            return None;
        };
        let temp = TempId(def.index());
        if lowering.bindings.fixed_temps.get(def.index()) != Some(&temp)
            || lowering.bindings.lvalue_for_temp(temp) != HirLValue::Temp(temp)
            || definition_is_captured(lowering, *def)
            || lowering
                .bindings
                .temp_debug_locals
                .get(temp.index())
                .is_some_and(Option::is_some)
            || !lowering.dataflow.def_phi_uses[def.index()].is_empty()
            || lowering.dataflow.def_uses[def.index()]
                .iter()
                .any(|site| !(start..end).contains(&site.instr.index()))
        {
            return None;
        }
    }

    let mut prefix = regular(lowering, start)?;
    prefix.extend(regular(lowering, start + 1)?);
    let [HirStmt::Assign(load), HirStmt::Assign(invoke)] = prefix.as_slice() else {
        return None;
    };
    let ([callee @ HirExpr::GlobalRef(_)], None) =
        (load.values.fixed.as_slice(), &load.values.tail)
    else {
        return None;
    };
    let ([HirLValue::Temp(condition)], [HirExpr::Call(call_expr)], None) = (
        invoke.targets.as_slice(),
        invoke.values.fixed.as_slice(),
        &invoke.values.tail,
    ) else {
        return None;
    };
    let mut then_stmts = regular(lowering, start + 3)?;
    then_stmts.extend(regular(lowering, start + 4)?);
    let [HirStmt::Assign(create), HirStmt::Assign(install)] = then_stmts.as_slice() else {
        return None;
    };
    let ([closure @ HirExpr::Closure(_)], None) =
        (create.values.fixed.as_slice(), &create.values.tail)
    else {
        return None;
    };
    if !matches!(install.targets.as_slice(), [HirLValue::Global(_)])
        || stmts.get(..prefix.len()) != Some(prefix.as_slice())
    {
        return None;
    }
    let HirStmt::If(guard) = stmts.get(prefix.len())? else {
        return None;
    };
    if guard.cond != HirExpr::TempRef(*condition)
        || guard.else_block.is_some()
        || guard.then_block.stmts != then_stmts
    {
        return None;
    }
    let mut condition = call_expr.clone();
    condition.callee = callee.clone();
    let mut install = install.clone();
    install.values = vec![closure.clone()].into();
    Some((
        HirStmt::If(Box::new(HirIf {
            cond: HirExpr::Call(condition),
            then_block: HirBlock {
                stmts: vec![HirStmt::Assign(install)],
            },
            else_block: None,
        })),
        prefix.len() + 1,
        ClosedArrayRegion {
            start,
            end,
            root: get.dst,
            retained: false,
        },
    ))
}

fn regular(lowering: &ProtoLowering<'_>, pc: usize) -> Option<Vec<HirStmt>> {
    lower_regular_instr(
        lowering,
        lowering.cfg.instr_to_block[pc],
        InstrRef(pc),
        lowering.proto.instrs.get(pc)?,
    )
}
