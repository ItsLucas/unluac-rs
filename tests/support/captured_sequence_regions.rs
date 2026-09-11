//! 连续构造声明必须同时保留捕获身份、原始帧和每个临时槽的覆盖顺序。
use super::*;

const SOURCE: &str = include_str!("../regress-case/regress_420_captured_constructor_sequence.lua");

fn whole_builder(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

#[test]
fn regressions_420_captured_sequence_keeps_identity_and_physical_writes() {
    assert_roundtrip_trace(&Workspace::new(), SOURCE, whole_builder);
}

#[test]
fn regressions_420_closed_batches_and_primitive_slots() {
    let workspace = Workspace::new();
    for count in [1, 49, 50, 51, 100, 101] {
        let values = (0..count)
            .map(|index| match index % 4 {
                0 => "nil".to_owned(),
                1 => "true".to_owned(),
                2 => "false".to_owned(),
                _ => index.to_string(),
            })
            .collect::<Vec<_>>()
            .join(",");
        let source = SOURCE.replace(
            "local first = {[10] = {value = 2}, [20] = {value = 3}}",
            &format!("local batch = {{{values}}}\n    local first = {{[10] = {{value = 2}}, [20] = {{value = 3}}}}"),
        ).replace(
            "function Seq420Read() return count, first, second end",
            "function Seq420Read() assert(batch[1] == nil) return count, first, second end",
        );
        assert_roundtrip_trace(&workspace, &source, whole_builder);
    }
}

#[test]
fn regressions_420_open_results_then_captured_declarations_keep_the_frame() {
    let workspace = Workspace::new();
    let source = include_str!("../regress-case/regress_414_captured_open_tail.lua");
    for after in ["{}", "{false, nil, 3}", "{{nil, 2}, {3, nil}}"] {
        let changed = source.replace(
            "function Open414Read() return tag, rows end",
            &format!(
                "local after = {after}\n    function Open414Read() assert(type(after) == 'table') return tag, rows end\n    Open414Make(3)"
            ),
        );
        assert_roundtrip_trace(&workspace, &changed, whole_builder);
    }
    let changed = source.replace(
        "function Open414Read() return tag, rows end",
        "function Open414Read() return tag, rows end\n    Open414Make(3)",
    );
    assert_roundtrip_trace(&workspace, &changed, whole_builder);
}

#[test]
fn regressions_420_open_suffix_statements_preserve_the_complete_frame() {
    let workspace = Workspace::new();
    let source = include_str!("../regress-case/regress_414_captured_open_tail.lua");
    for suffix in [
        "local unobserved = Open414Unknown",
        "if Open414Flag then Open414Make(3) end",
        "Open414Make(Open414Make(3))",
    ] {
        let changed = source.replace(
            "function Open414Read() return tag, rows end",
            &format!("{suffix}\n function Open414Read() return tag, rows end"),
        );
        for flag in ["false", "true"] {
            let changed = format!("Open414Fail = -1\nOpen414Flag = {flag}\n{changed}");
            assert_roundtrip_trace(&workspace, &changed, whole_builder);
        }
    }
}

#[test]
fn regressions_429_global_constructor_before_retained_call_anchor() {
    assert_roundtrip_trace(
        &Workspace::new(),
        include_str!("../regress-case/regress_429_global_then_retained_call.lua"),
        whole_builder,
    );
}
