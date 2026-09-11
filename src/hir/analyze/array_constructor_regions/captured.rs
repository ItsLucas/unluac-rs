//! 在完整建表区间内保留既有捕获根的声明与 upvalue 身份。
//!
//! 本模块消费原始 SSA 与已经闭合的前序构造器：`local a={...}; local b={...}`
//! 可连续恢复，每个根准确占一槽，临时定义只有自身从未捕获且无区间外使用才可消除。
//! 后来的另一个根复用同一物理槽，不应反向把早先临时值当成捕获对象。
//! 未证明的前序语句仍由入口帧门拒绝；不会将已有开放 upvalue 的赋值改成声明。

use super::*;

pub(super) fn captured_array_region(
    stmts: &[HirStmt],
    lowering: &ProtoLowering<'_>,
    closed_arrays: &[ClosedArrayRegion],
) -> Option<(HirStmt, usize, ClosedArrayRegion)> {
    let (target, values) = match stmts.first()? {
        HirStmt::LocalDecl(seed) if seed.bindings.len() == 1 => {
            (HirLValue::Local(seed.bindings[0]), &seed.values)
        }
        HirStmt::Assign(seed) => {
            let [target @ (HirLValue::Local(_) | HirLValue::Temp(_))] = seed.targets.as_slice()
            else {
                return None;
            };
            (target.clone(), &seed.values)
        }
        _ => return None,
    };
    let ([HirExpr::TableConstructor(table)], None) = (values.fixed.as_slice(), &values.tail) else {
        return None;
    };
    if !table.fields.is_empty() || table.trailing_multivalue.is_some() {
        return None;
    }
    // Immutable captures still refer to SSA temps at this stage; mutable ones
    // already have bound locals. Preserve either identity for later promotion.
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
    let mut roots = candidates.into_iter().filter(|temp| {
        let Some(definition) = lowering.dataflow.defs.get(temp.index()) else {
            return false;
        };
        let Some(LowInstr::NewTable(seed)) = lowering.proto.instrs.get(definition.instr.index())
        else {
            return false;
        };
        prefix_with_closed_arrays(
            lowering.proto,
            lowering.cfg,
            lowering.dataflow,
            definition.instr.index(),
            seed.dst,
            closed_arrays,
        )
    });
    let root = roots.next()?;
    if roots.next().is_some() {
        return None;
    }
    let definition = lowering.dataflow.defs.get(root.index())?;
    let start = definition.instr.index();
    let LowInstr::NewTable(new_table) = lowering.proto.instrs.get(start)? else {
        return None;
    };
    if lowering.bindings.fixed_temps.get(definition.id.index()) != Some(&root)
        || lowering.bindings.lvalue_for_temp(root) != target
        || !lowering.bindings.reg_is_reference_captured(new_table.dst)
        || !lowering.dataflow.def_phi_uses[definition.id.index()].is_empty()
        || !prefix_with_closed_arrays(
            lowering.proto,
            lowering.cfg,
            lowering.dataflow,
            start,
            new_table.dst,
            closed_arrays,
        )
    {
        return None;
    }

    let mut parser = ArrayParser {
        lowering,
        block: definition.block,
        root: new_table.dst,
        start,
        cursor: start,
        has_observable_producer: false,
        allow_open_tail: true,
        rk_pool_full: false,
    };
    let expression = parser.table(0)?;
    let end = parser.cursor;
    // A later capture of a different definition in this physical register is not
    // evidence that this constructor root will retain a source-local home.
    if !lowering.dataflow.def_uses[definition.id.index()]
        .iter()
        .any(|site| {
            site.instr.index() >= end
                && matches!(&lowering.proto.instrs[site.instr.index()], LowInstr::Closure(closure)
                    if closure.captures.iter().any(|capture|
                        capture.source == crate::transformer::CaptureSource::ByReference(new_table.dst)))
        })
    {
        return None;
    }
    let mut expected = Vec::new();
    for pc in start..end {
        for def in &lowering.dataflow.instr_defs[pc] {
            if *def == definition.id {
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
                    .any(|site| !(start..end).contains(&site.instr.index()))
            {
                return None;
            }
        }
        expected.extend(lower_regular_instr(
            lowering,
            definition.block,
            InstrRef(pc),
            &lowering.proto.instrs[pc],
        )?);
    }
    if stmts.get(..expected.len()) != Some(expected.as_slice()) {
        return None;
    }
    let mut replacement = stmts.first()?.clone();
    match &mut replacement {
        HirStmt::LocalDecl(decl) => decl.values = vec![expression].into(),
        HirStmt::Assign(assign) => assign.values = vec![expression].into(),
        _ => return None,
    }
    Some((
        replacement,
        expected.len(),
        ClosedArrayRegion {
            start,
            end,
            root: new_table.dst,
            retained: true,
        },
    ))
}
