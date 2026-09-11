//! 恢复捕获标量的完整查表栈，同时保留最终声明的 SSA／局部身份。
//!
//! 本模块依赖原始 HIR 区间逐项匹配、定义级捕获事实和已证明前缀帧。例如
//! `local sort = table.sort` 保留 sort 的原槽，并按原顺序消费高 RK 字符串键。
//! 仅接受全局与字符串查表链；未知参数计算、动态键和中间定义逃逸继续拒绝。

use super::*;
use crate::hir::common::HirTableAccess;
use crate::transformer::CaptureSource;

pub(super) fn captured_value_region(
    stmts: &[HirStmt],
    lowering: &ProtoLowering<'_>,
    closed: &[ClosedArrayRegion],
) -> Option<(HirStmt, usize, ClosedArrayRegion)> {
    let (target, values) = match stmts.first()? {
        HirStmt::Assign(assign) if assign.targets.len() == 1 => {
            (assign.targets[0].clone(), &assign.values)
        }
        HirStmt::LocalDecl(decl) if decl.bindings.len() == 1 => {
            (HirLValue::Local(decl.bindings[0]), &decl.values)
        }
        _ => return None,
    };
    if values.fixed.len() != 1 || values.tail.is_some() {
        return None;
    }
    let candidates = match target {
        HirLValue::Temp(temp) => vec![temp],
        HirLValue::Local(_) => lowering
            .bindings
            .captured_temp_targets
            .iter()
            .filter_map(|(temp, bound)| (bound.lvalue() == target).then_some(*temp))
            .collect(),
        _ => return None,
    };
    let mut candidates = candidates.into_iter().filter_map(|temp| {
        let definition = lowering.dataflow.defs.get(temp.index())?;
        let start = definition.instr.index();
        let LowInstr::GetTable(get) = lowering.proto.instrs.get(start)? else {
            return None;
        };
        (get.kind == GetTableKind::Normal
            && get.base == AccessBase::Env
            && matches!(get.key, AccessKey::Const(key)
                if matches!(expr_for_const(lowering.proto, key), HirExpr::String(_)))
            && prefix_with_closed_arrays(
                lowering.proto,
                lowering.cfg,
                lowering.dataflow,
                start,
                get.dst,
                closed,
            ))
        .then_some((start, get.dst, definition.block))
    });
    let (start, root, block) = candidates.next()?;
    if candidates.next().is_some() {
        return None;
    }
    let mut expression = scalar(lowering, block, start)?;
    let mut final_pc = start;
    let mut cursor = start + 1;
    loop {
        if cursor - start >= MAX_REGION_INSTRS
            || lowering.cfg.instr_to_block.get(cursor) != Some(&block)
        {
            return None;
        }
        match lowering.proto.instrs.get(cursor)? {
            LowInstr::GetTable(get)
                if get.kind == GetTableKind::Normal
                    && get.dst == root
                    && get.base == AccessBase::Reg(root) =>
            {
                let AccessKey::Const(key) = get.key else {
                    return None;
                };
                if key.index() > 255 {
                    return None;
                }
                // String keys already in RK retain their index even after the
                // numeric literal pool is full. Other key expression forms need
                // a separate stack proof.
                let key = expr_for_const(lowering.proto, key);
                if !matches!(key, HirExpr::String(_)) {
                    return None;
                }
                expression = HirExpr::TableAccess(Box::new(HirTableAccess {
                    base: expression,
                    key,
                }));
                final_pc = cursor;
                cursor += 1;
            }
            LowInstr::LoadConst(load) if load.dst.index() == root.index() + 1 => {
                let key = expr_for_const(lowering.proto, load.value);
                if load.value.index() <= 255 || !matches!(key, HirExpr::String(_)) {
                    return None;
                }
                let Some(LowInstr::GetTable(get)) = lowering.proto.instrs.get(cursor + 1) else {
                    return None;
                };
                if lowering.cfg.instr_to_block.get(cursor + 1) != Some(&block)
                    || get.kind != GetTableKind::Normal
                    || get.dst != root
                    || get.base != AccessBase::Reg(root)
                    || get.key != AccessKey::Reg(load.dst)
                {
                    return None;
                }
                expression = HirExpr::TableAccess(Box::new(HirTableAccess {
                    base: expression,
                    key,
                }));
                final_pc = cursor + 1;
                cursor += 2;
            }
            _ => break,
        }
    }
    let [final_def] = lowering.dataflow.instr_defs.get(final_pc)?.as_slice() else {
        return None;
    };
    let final_temp = TempId(final_def.index());
    if lowering.dataflow.def_reg(*final_def) != root
        || lowering.bindings.fixed_temps.get(final_def.index()) != Some(&final_temp)
        || !lowering.dataflow.def_phi_uses[final_def.index()].is_empty()
        || !lowering.dataflow.def_uses[final_def.index()].iter().any(|site|
            site.instr.index() >= cursor
                && matches!(&lowering.proto.instrs[site.instr.index()], LowInstr::Closure(closure)
                    if closure.captures.iter().any(|capture| capture.source == CaptureSource::ByReference(root))))
    { return None; }
    let mut expected = Vec::new();
    for pc in start..cursor {
        for def in &lowering.dataflow.instr_defs[pc] {
            if def == final_def {
                continue;
            }
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
                    .any(|site| !(start..cursor).contains(&site.instr.index()))
            {
                return None;
            }
        }
        expected.extend(lower_regular_instr(
            lowering,
            block,
            InstrRef(pc),
            &lowering.proto.instrs[pc],
        )?);
    }
    if stmts.get(..expected.len()) != Some(expected.as_slice()) {
        return None;
    }
    let mut replacement = expected.last()?.clone();
    match &mut replacement {
        HirStmt::LocalDecl(decl)
            if decl.bindings.len() == 1
                && lowering.bindings.lvalue_for_temp(final_temp)
                    == HirLValue::Local(decl.bindings[0]) =>
        {
            decl.values = vec![expression].into()
        }
        HirStmt::Assign(assign)
            if assign.targets == [lowering.bindings.lvalue_for_temp(final_temp)] =>
        {
            assign.values = vec![expression].into()
        }
        _ => return None,
    }
    Some((
        replacement,
        expected.len(),
        ClosedArrayRegion {
            start,
            end: cursor,
            root,
            retained: true,
        },
    ))
}

fn scalar(lowering: &ProtoLowering<'_>, block: BlockRef, pc: usize) -> Option<HirExpr> {
    let stmts = lower_regular_instr(
        lowering,
        block,
        InstrRef(pc),
        lowering.proto.instrs.get(pc)?,
    )?;
    let values = match stmts.as_slice() {
        [HirStmt::Assign(assign)] if assign.targets.len() == 1 => &assign.values,
        [HirStmt::LocalDecl(decl)] if decl.bindings.len() == 1 => &decl.values,
        _ => return None,
    };
    let ([expression], None) = (values.fixed.as_slice(), &values.tail) else {
        return None;
    };
    Some(expression.clone())
}
