//! 从官方源码编译取得 SSA 前缀，校验不可保留的调用根不会进入构造器帧证明。
use super::*;
use crate::decompile::{DecompileOptions, DecompileStage, GenerateMode, decompile};

#[test]
fn prefix_proof_rejects_unread_or_single_use_call_roots_and_shifted_frames() {
    let bytes = unluac_test_support::compile_lua_case(
        "lua5.1",
        "tests/regress-case/regress_397_effectful_array_regions.lua",
    );
    let mut options = DecompileOptions {
        target_stage: DecompileStage::Structure,
        ..DecompileOptions::default()
    };
    options.generate.mode = GenerateMode::Strict;
    let state = decompile(&bytes, options).unwrap().state;
    let lowered = state.lowered.unwrap();
    let index = lowered
        .main
        .children
        .iter()
        .position(|proto| {
            proto
                .instrs
                .iter()
                .any(|instr| matches!(instr, LowInstr::SetList(_)))
                && proto.instrs.iter().any(|instr| {
                    matches!(instr,
                LowInstr::SetTable(store) if store.base == AccessBase::Env)
                })
        })
        .expect("original builder");
    let proto = &lowered.main.children[index];
    let cfg = &state.cfg.as_ref().unwrap().children[index].cfg;
    let dataflow = &state.dataflow.as_ref().unwrap().children[index];
    let start = proto
        .instrs
        .iter()
        .position(|instr| matches!(instr, LowInstr::NewTable(_)))
        .unwrap();
    let LowInstr::NewTable(root) = proto.instrs[start] else {
        unreachable!();
    };
    assert!(prefix_with_closed_arrays(
        proto,
        cfg,
        dataflow,
        start,
        root.dst,
        &[]
    ));
    assert!(!prefix_with_closed_arrays(
        proto,
        cfg,
        dataflow,
        start,
        Reg(root.dst.index() + 1),
        &[]
    ));

    let call = proto.instrs[..start]
        .iter()
        .position(|instr| matches!(instr, LowInstr::Call(_)))
        .unwrap();
    let def = dataflow.instr_defs[call][0];
    let mut fewer_reads = dataflow.clone();
    fewer_reads.def_uses[def.index()].truncate(1);
    assert!(!prefix_with_closed_arrays(
        proto,
        cfg,
        &fewer_reads,
        start,
        root.dst,
        &[]
    ));
    fewer_reads.def_uses[def.index()].clear();
    assert!(!prefix_with_closed_arrays(
        proto,
        cfg,
        &fewer_reads,
        start,
        root.dst,
        &[]
    ));
}
