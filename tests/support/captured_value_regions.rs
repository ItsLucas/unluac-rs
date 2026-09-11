//! 捕获 getter 链须保留调用、环境读取、物理覆盖和闭包绑定，动态键与后续写回也核对全帧。
use super::*;

const SOURCE: &str = include_str!("../regress-case/regress_426_captured_lookup_regions.lua");

fn whole_builder(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

#[test]
fn regressions_426_captured_lookup_stack_preserves_identity_and_gc() {
    assert_roundtrip_trace(&Workspace::new(), SOURCE, whole_builder);
}

#[test]
fn regressions_426_captured_lookup_stack_keeps_large_string_key_slots() {
    let workspace = Workspace::new();
    for count in [248, 254, 256, 270] {
        let mut prefix = String::new();
        for value in 0..count {
            writeln!(prefix, "Capture426Load(\"pool426_{value}\")").unwrap();
        }
        let source = SOURCE.replace("Capture426Load(\"prefix\")", &prefix);
        assert_roundtrip_trace(&workspace, &source, whole_builder);
    }
}

#[test]
fn regressions_426_captured_call_lookup_keeps_frame_and_gc_order() {
    let source = SOURCE
        .replace(
            "local sort = Capture426Library.sort",
            "local sort = Capture426MakeLibrary().sort",
        )
        .replace(
            "function Capture426Build(marker)",
            r#"
function Capture426MakeLibrary()
    event("make-library")
    local library = setmetatable({}, {
        __index = function(_, name) event("call-field:" .. name); return table[name] end,
    })
    weak.library = library
    return library
end
function Capture426Build(marker)"#,
        );
    assert_roundtrip_trace(&Workspace::new(), &source, whole_builder);
}

#[test]
fn regressions_426_captured_dynamic_lookup_and_assignment_preserve_the_frame() {
    let workspace = Workspace::new();
    let source = SOURCE.replace(
        "function Capture426Build(marker)",
        "Capture426Dynamic = \"sort\"\nCapture426Other = table.sort\nfunction Capture426Build(marker)",
    );
    for source in [
        source.replace(
            "local sort = Capture426Library.sort",
            "local sort = Capture426Library[Capture426Dynamic]",
        ),
        source.replace(
            "local sort = Capture426Library.sort",
            "local sort = Capture426Library.sort\n    sort = Capture426Other",
        ),
    ] {
        assert_roundtrip_trace(&workspace, &source, whole_builder);
    }
}
