use super::*;
use crate::decompile::{DecompileOptions, DecompileStage, decompile};

#[test]
fn ignored_calls_keep_the_frame_but_open_or_unread_results_do_not() {
    let bytes = unluac_test_support::compile_lua_case(
        "lua5.1",
        "tests/regress-case/regress_398_record_lookup_regions.lua",
    );
    let mut options = DecompileOptions::default();
    options.target_stage = DecompileStage::Structure;
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
        .unwrap();
    let mut proto = (*lowered.main.children[index]).clone();
    let cfg = &state.cfg.as_ref().unwrap().children[index].cfg;
    let dataflow = &state.dataflow.as_ref().unwrap().children[index];
    let end = proto
        .instrs
        .iter()
        .position(|instr| matches!(instr, LowInstr::NewTable(_)))
        .unwrap();
    let LowInstr::NewTable(root) = proto.instrs[end] else {
        unreachable!()
    };
    assert!(canonical_prefix_keeps_frame(
        &proto, cfg, dataflow, end, root.dst
    ));
    let call_pc = proto
        .instrs
        .iter()
        .position(|instr| matches!(instr, LowInstr::Call(_)))
        .unwrap();
    let LowInstr::Call(mut call) = proto.instrs[call_pc] else {
        unreachable!()
    };
    assert_eq!(call.results, ResultPack::Ignore);
    call.results = ResultPack::Open(call.callee);
    proto.instrs[call_pc] = LowInstr::Call(call);
    assert!(!canonical_prefix_keeps_frame(
        &proto, cfg, dataflow, end, root.dst
    ));
    call.results = ResultPack::Fixed(crate::transformer::RegRange::new(call.callee, 1));
    proto.instrs[call_pc] = LowInstr::Call(call);
    assert!(!canonical_prefix_keeps_frame(
        &proto, cfg, dataflow, end, root.dst
    ));
}
