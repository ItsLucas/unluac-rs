//! 动态调用键和闭包字段共享完整构造事务，检查GC、异常、捕获身份与完整指令轨迹。
use super::*;

const SOURCE: &str = include_str!("../regress-case/regress_423_record_closures_call_keys.lua");

fn whole_builder(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

fn check(workspace: &Workspace, source: &str) {
    assert_roundtrip_trace(workspace, source, whole_builder);
}

fn source_with_prefix(prefix: usize) -> String {
    let mut loads = String::new();
    for index in 0..prefix {
        writeln!(loads, "Dynamic423Load(\"pool423_{index}\")").unwrap();
    }
    SOURCE.replace("Dynamic423Load(\"prefix\")", &loads)
}

#[test]
fn regressions_423_call_keys_and_closures_preserve_stack_and_capture_identity() {
    let workspace = Workspace::new();
    for prefix in [0, 248, 254, 256, 270] {
        check(&workspace, &source_with_prefix(prefix));
    }
}

#[test]
fn regressions_423_repeated_call_keys_preserve_overwrite_and_gc_order() {
    let workspace = Workspace::new();
    for prefix in [0, 270] {
        let source = source_with_prefix(prefix)
            .replace(
                "local key = {index = index, label = label}",
                "local key = \"same\"",
            )
            .replace(
                "Dynamic423Root[weak.k1].items[1].value.index == 1",
                "Dynamic423Root[weak.k1].items[1].value.index == 3",
            )
            .replace(
                "Dynamic423Root[weak.k2].items[1].value.index == 2",
                "Dynamic423Root[weak.k2].items[1].value.index == 3",
            );
        check(&workspace, &source);
    }
}

#[test]
fn regressions_423_numeric_call_results_keep_record_allocation() {
    let workspace = Workspace::new();
    for prefix in [0, 270] {
        check(
            &workspace,
            &source_with_prefix(prefix).replace(
                "local key = {index = index, label = label}",
                "local key = index",
            ),
        );
    }
}

#[test]
fn regressions_423_call_key_direct_arrays_close_at_exact_parent_stores() {
    let workspace = Workspace::new();
    for items in [0, 1, 49, 50, 51, 100] {
        let mut values = String::new();
        for index in 1..=items {
            writeln!(values, "(Dynamic423Value({index})),").unwrap();
        }
        let source = format!(
            r#"
local weak = setmetatable({{}}, {{__mode = "v"}})
local trace = {{}}
function Dynamic423Key(index)
    collectgarbage("collect")
    trace[#trace + 1] = "key:" .. index .. ":" .. (weak[index - 1] and "live" or "dead")
    local key = {{index}}
    weak[index] = key
    return key, "discarded"
end
function Dynamic423Value(index)
    collectgarbage("collect")
    trace[#trace + 1] = "value:" .. index .. ":" .. (weak[1] and "live" or "dead")
    return index
end
function Dynamic423Build()
    Dynamic423Root = {{
        [Dynamic423Key(1)] = {{{values}}},
        [Dynamic423Key(2)] = {{{values}}},
    }}
end
Dynamic423Build()
assert(#Dynamic423Root[weak[1]] == {items} and #Dynamic423Root[weak[2]] == {items})
for index = 1, {items} do
    assert(Dynamic423Root[weak[1]][index] == index)
    assert(Dynamic423Root[weak[2]][index] == index)
end
print(table.concat(trace, "|"))
"#,
        );
        check(&workspace, &source);
    }
}

#[test]
fn regressions_423_other_dynamic_key_producers_stay_outside_the_call_proof() {
    let workspace = Workspace::new();
    for key in [
        "Dynamic423Unknown",
        "Dynamic423Key(Dynamic423Unknown, \"first\")",
        "Dynamic423Key(1, \"first\") + 0",
    ] {
        let source = SOURCE.replace("Dynamic423Key(1, \"first\")", key);
        for strip in [true, false] {
            let bytes = compile(&workspace, &source, strip);
            let error = decompile(&bytes, options(NamingMode::Simple))
                .expect_err("unproved key expression must preserve strict rejection");
            assert!(error.to_string().contains("residual"), "{error}");
        }
    }
}

#[test]
fn regressions_423_changed_key_slots_and_result_width_stay_rejected() {
    let workspace = Workspace::new();
    for strip in [true, false] {
        let bytes = compile(&workspace, SOURCE, strip);
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.unwrap();
        let index = lowered
            .main
            .children
            .iter()
            .position(|proto| prefix_and_region(proto).is_some())
            .unwrap();
        let proto = &lowered.main.children[index];
        let (call_pc, call, child) = proto
            .instrs
            .windows(2)
            .enumerate()
            .find_map(|(pc, pair)| match pair {
                [LowInstr::Call(call), LowInstr::NewTable(child)]
                    if matches!(call.results, unluac::transformer::ResultPack::Fixed(_)) =>
                {
                    Some((pc, call, child))
                }
                _ => None,
            })
            .unwrap();
        let store_pc = proto.instrs[call_pc..]
            .iter()
            .position(|instruction| {
                matches!(instruction, LowInstr::SetTable(store)
                if store.key == AccessKey::Reg(call.callee)
                    && store.value == ValueOperand::Reg(child.dst))
            })
            .unwrap()
            + call_pc;
        let raw = result.state.raw_chunk.unwrap();
        for (pc, shift, width, value) in [
            (call_pc, 14, 0x1ff, 0),
            (call_pc, 14, 0x1ff, 3),
            (call_pc + 1, 6, 0xff, call.callee.index() as u32),
            (store_pc, 23, 0x1ff, child.dst.index() as u32),
        ] {
            let raw_index = proto.lowering_map.low_to_raw[pc][0].index();
            let origin = raw.main.common.children[index].common.instructions[raw_index].origin;
            let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
            let patched = (word & !(width << shift)) | (value << shift);
            let error = rejected_record_word(&bytes, origin.span.offset, patched);
            assert!(error.contains("residual"), "{error}");
        }
    }
}

#[test]
fn regressions_423_closure_capture_of_a_region_slot_stays_rejected() {
    let workspace = Workspace::new();
    for strip in [true, false] {
        let bytes = compile(&workspace, SOURCE, strip);
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let lowered = result.state.lowered.unwrap();
        let index = lowered
            .main
            .children
            .iter()
            .position(|proto| prefix_and_region(proto).is_some())
            .unwrap();
        let proto = &lowered.main.children[index];
        let root = proto
            .instrs
            .iter()
            .find_map(|instruction| match instruction {
                LowInstr::NewTable(table) => Some(table.dst),
                _ => None,
            })
            .unwrap();
        let (pc, closure) = proto
            .instrs
            .iter()
            .enumerate()
            .find_map(|(pc, instruction)| match instruction {
                LowInstr::Closure(closure) => Some((pc, closure)),
                _ => None,
            })
            .unwrap();
        assert!(matches!(closure.captures[0].source,
            unluac::transformer::CaptureSource::ByReference(reg) if reg.index() < root.index()));
        let capture_index = proto.lowering_map.low_to_raw[pc][1].index();
        let raw = result.state.raw_chunk.unwrap();
        let origin = raw.main.common.children[index].common.instructions[capture_index].origin;
        let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
        let patched = (word & !(0x1ff << 23)) | ((root.index() as u32) << 23);
        let error = rejected_record_word(&bytes, origin.span.offset, patched);
        assert!(error.contains("residual"), "{error}");
    }
}
