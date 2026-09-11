//! 为规范 pairs(global) 循环生成保持外层局部栈的闭合区间证据。
//!
//! 原始调用／循环协议必须与既有 GenericFor 节点、SSA 使用范围和前缀帧一致。
//! 例如 `for k,v in pairs(G) do G[k]=v*Scale end` 保留原循环绑定，只收回其准备
//! 与循环体临时值。仅支持两个绑定及直接值或全局乘数写回；低槽写入、额外控制流
//! 和逃逸临时值继续拒绝，最终提交仍由父模块帧账本决定。

use super::*;
use crate::hir::common::{
    HirAssign, HirBinaryExpr, HirGenericFor, HirPackTail, HirTableAccess, HirValuePack,
};
use crate::structure::PhiId;
use std::collections::BTreeSet;

pub(super) fn loop_region(
    stmts: &[HirStmt],
    lowering: &ProtoLowering<'_>,
    closed: &[ClosedArrayRegion],
) -> Option<(HirStmt, usize, ClosedArrayRegion)> {
    let HirStmt::Assign(seed) = stmts.first()? else {
        return None;
    };
    let ([HirLValue::Temp(temp)], [_], None) = (
        seed.targets.as_slice(),
        seed.values.fixed.as_slice(),
        &seed.values.tail,
    ) else {
        return None;
    };
    let definition = lowering.dataflow.defs.get(temp.index())?;
    let start = definition.instr.index();
    let root = lowering.dataflow.def_reg(definition.id);
    if !prefix_with_closed_arrays(
        lowering.proto,
        lowering.cfg,
        lowering.dataflow,
        start,
        root,
        closed,
    ) {
        return None;
    }
    let callee = global_read(lowering, start, root)?;
    if !matches!(callee, HirExpr::GlobalRef(ref name) if name.name == "pairs") {
        return None;
    }
    let argument = global_read(lowering, start + 1, Reg(root.index() + 1))?;
    let LowInstr::Call(call) = *lowering.proto.instrs.get(start + 2)? else {
        return None;
    };
    if call.kind != CallKind::Normal
        || call.method_name.is_some()
        || call.callee != root
        || !matches!(call.args, ValuePack::Fixed(args) if args.start.index() == root.index() + 1 && args.len == 1)
        || !matches!(call.results, ResultPack::Fixed(results) if results.start == root && results.len == 3)
    {
        return None;
    }
    let LowInstr::Jump(jump) = *lowering.proto.instrs.get(start + 3)? else {
        return None;
    };
    let control = jump.target.index();
    let body_start = start + 4;
    if control != body_start + 2 && control != body_start + 4 {
        return None;
    }
    let LowInstr::GenericForCall(iterate) = *lowering.proto.instrs.get(control)? else {
        return None;
    };
    let LowInstr::GenericForLoop(loop_) = *lowering.proto.instrs.get(control + 1)? else {
        return None;
    };
    let end = control + 2;
    let key_reg = Reg(root.index() + 3);
    let value_reg = Reg(root.index() + 4);
    if iterate.iterator != root
        || iterate.state.index() != root.index() + 1
        || iterate.control.index() != root.index() + 2
        || !matches!(iterate.results, ResultPack::Fixed(results) if results.start == key_reg && results.len == 2)
        || loop_.control_target != iterate.control
        || loop_.bindings.start != key_reg
        || loop_.bindings.len != 2
        || loop_.body_target.index() != body_start
        || loop_.exit_target.index() != end
    {
        return None;
    }
    let preheader = definition.block;
    if lowering
        .cfg
        .instr_to_block
        .get(start..start + 4)?
        .iter()
        .any(|block| *block != preheader)
    {
        return None;
    }
    let body_block = *lowering.cfg.instr_to_block.get(body_start)?;
    if lowering
        .cfg
        .instr_to_block
        .get(body_start..control)?
        .iter()
        .any(|block| *block != body_block)
    {
        return None;
    }
    let key_local = lowering
        .bindings
        .local_for_reg_in_block(body_block, key_reg)?;
    let value_local = lowering
        .bindings
        .local_for_reg_in_block(body_block, value_reg)?;
    if key_local == value_local {
        return None;
    }
    let target_reg = Reg(root.index() + 5);
    let target = global_read(lowering, body_start, target_reg)?;
    let value = if control == body_start + 2 {
        HirExpr::LocalRef(value_local)
    } else {
        let factor_reg = Reg(root.index() + 6);
        let factor = global_read(lowering, body_start + 1, factor_reg)?;
        let LowInstr::BinaryOp(binary) = *lowering.proto.instrs.get(body_start + 2)? else {
            return None;
        };
        if binary.op != crate::transformer::BinaryOpKind::Mul
            || binary.dst != factor_reg
            || binary.lhs != ValueOperand::Reg(value_reg)
            || binary.rhs != ValueOperand::Reg(factor_reg)
        {
            return None;
        }
        HirExpr::Binary(Box::new(HirBinaryExpr {
            operand_order: None,
            op: HirBinaryOpKind::Mul,
            lhs: HirExpr::LocalRef(value_local),
            rhs: factor,
        }))
    };
    let LowInstr::SetTable(store) = *lowering.proto.instrs.get(control - 1)? else {
        return None;
    };
    let rhs_reg = if control == body_start + 2 {
        value_reg
    } else {
        Reg(root.index() + 6)
    };
    if store.kind != SetTableKind::Normal
        || store.base != AccessBase::Reg(target_reg)
        || store.key != AccessKey::Reg(key_reg)
        || store.value != ValueOperand::Reg(rhs_reg)
    {
        return None;
    }
    if !private_definitions(lowering, start, end, root) {
        return None;
    }

    let mut expected_prefix = Vec::new();
    for pc in start..start + 3 {
        expected_prefix.extend(lower_regular_instr(
            lowering,
            preheader,
            InstrRef(pc),
            &lowering.proto.instrs[pc],
        )?);
    }
    if stmts.get(..expected_prefix.len()) != Some(expected_prefix.as_slice()) {
        return None;
    }
    let HirStmt::GenericFor(original) = stmts.get(expected_prefix.len())? else {
        return None;
    };
    if original.bindings != [key_local, value_local] {
        return None;
    }
    let expected_iterator: Vec<_> = (0..3)
        .map(|offset| {
            super::super::exprs::expr_for_reg_at_block_exit(
                lowering,
                preheader,
                Reg(root.index() + offset),
            )
        })
        .collect();
    if original.iterator != HirValuePack::fixed(expected_iterator) {
        return None;
    }
    let mut expected_body = Vec::new();
    for pc in body_start..control {
        expected_body.extend(lower_regular_instr(
            lowering,
            body_block,
            InstrRef(pc),
            &lowering.proto.instrs[pc],
        )?);
    }
    if original.body.stmts != expected_body {
        return None;
    }
    let iterator = HirExpr::Call(Box::new(HirCallExpr {
        callee,
        args: vec![argument].into(),
        method: false,
        fastcall: None,
        method_name: None,
    }));
    let replacement = HirStmt::GenericFor(Box::new(HirGenericFor {
        bindings: original.bindings.clone(),
        iterator: HirValuePack::expanding(Vec::new(), HirPackTail::open(iterator)),
        body: HirBlock {
            stmts: vec![HirStmt::Assign(Box::new(HirAssign {
                targets: vec![HirLValue::TableAccess(Box::new(HirTableAccess {
                    base: target,
                    key: HirExpr::LocalRef(key_local),
                }))],
                values: vec![value].into(),
            }))],
        },
    }));
    Some((
        replacement,
        expected_prefix.len() + 1,
        ClosedArrayRegion {
            start,
            end,
            root,
            retained: false,
        },
    ))
}

fn global_read(lowering: &ProtoLowering<'_>, pc: usize, dst: Reg) -> Option<HirExpr> {
    let LowInstr::GetTable(get) = lowering.proto.instrs.get(pc)? else {
        return None;
    };
    let AccessKey::Const(key) = get.key else {
        return None;
    };
    if get.kind != GetTableKind::Normal
        || get.base != AccessBase::Env
        || get.dst != dst
        || !matches!(expr_for_const(lowering.proto, key), HirExpr::String(_))
    {
        return None;
    }
    let block = *lowering.cfg.instr_to_block.get(pc)?;
    let statements =
        lower_regular_instr(lowering, block, InstrRef(pc), &lowering.proto.instrs[pc])?;
    let [HirStmt::Assign(assign)] = statements.as_slice() else {
        return None;
    };
    let ([HirLValue::Temp(_)], [expression], None) = (
        assign.targets.as_slice(),
        assign.values.fixed.as_slice(),
        &assign.values.tail,
    ) else {
        return None;
    };
    Some(expression.clone())
}

fn private_definitions(lowering: &ProtoLowering<'_>, start: usize, end: usize, root: Reg) -> bool {
    let blocks: BTreeSet<_> = lowering.cfg.instr_to_block[start..end]
        .iter()
        .copied()
        .collect();
    let mut phis: Vec<PhiId> = Vec::new();
    for pc in start..end {
        for def in &lowering.dataflow.instr_defs[pc] {
            if lowering.dataflow.def_reg(*def).index() < root.index()
                || definition_is_captured(lowering, *def)
                || lowering.dataflow.def_uses[def.index()]
                    .iter()
                    .any(|site| !(start..end).contains(&site.instr.index()))
            {
                return false;
            }
            phis.extend(&lowering.dataflow.def_phi_uses[def.index()]);
        }
    }
    let mut seen = BTreeSet::new();
    while let Some(phi) = phis.pop() {
        if !seen.insert(phi) || lowering.dataflow.phi_truly_dead[phi.index()] {
            continue;
        }
        if !blocks.contains(&lowering.dataflow.phi_candidates[phi.index()].block)
            || lowering.dataflow.phi_uses[phi.index()]
                .iter()
                .any(|site| !(start..end).contains(&site.instr.index()))
        {
            return false;
        }
        phis.extend(&lowering.dataflow.phi_phi_uses[phi.index()]);
    }
    true
}
