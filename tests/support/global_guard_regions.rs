//! 全局安装 guard 的控制、环境效果和槽生命期必须同时保持，后缀单独闭合。
use super::*;

const SOURCE: &str = include_str!("../regress-case/regress_427_global_guard_regions.lua");

fn whole_builder(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

fn check(workspace: &Workspace, source: &str) {
    assert_roundtrip_trace(workspace, source, whole_builder);
}

fn source_with_prefix(prefix: usize) -> String {
    let mut loads = String::new();
    for index in 0..prefix {
        writeln!(loads, "Guard427Load(\"pool427_{index}\")").unwrap();
    }
    SOURCE.replace("Guard427Load(\"prefix\")", &loads)
}

#[test]
fn regressions_427_global_guard_preserves_paths_gc_and_exceptions() {
    let workspace = Workspace::new();
    for prefix in [0, 248, 256, 270] {
        check(&workspace, &source_with_prefix(prefix));
    }
}

#[test]
fn regressions_427_guard_before_captured_declarations_keeps_the_successor_frame() {
    let workspace = Workspace::new();
    for prefix in [0, 270] {
        let source = source_with_prefix(prefix).replace(
            "        function Guard427Installed(value) return tag, rows, value end\n    end\nend",
            "        function Guard427Installed(value) return tag, rows, value end\n    end\n    local a = 23\n    local b = 29\n    local c = 31\n    local d = 37\n    function Guard427Read() return tag, rows, a, b, c, d end\nend",
        ).replace(
            "    Guard427Build()\n    if choice <= 1",
            "    Guard427Build()\n    local read_tag, read_rows, a, b, c, d = Guard427Read()\n    assert(read_tag == 17 and read_rows[1][1].index == 1 and a == 23 and b == 29 and c == 31 and d == 37)\n    if choice <= 1",
        );
        check(&workspace, &source);
    }
}

fn global_rows_source(prefix: usize) -> String {
    source_with_prefix(prefix)
        .replace("local rows =", "Guard427Rows =")
        .replace("return tag, rows, value", "return tag, Guard427Rows, value")
}

#[test]
fn regressions_427_guard_closes_the_preceding_global_constructor() {
    let workspace = Workspace::new();
    for prefix in [0, 270] {
        check(&workspace, &global_rows_source(prefix));
    }
}

#[test]
fn regressions_427_fixed_guard_arguments_inversion_and_call_suffix_keep_the_frame() {
    let workspace = Workspace::new();
    let source = global_rows_source(0);
    for changed in [
        source.replace("if Guard427Gate() then", "if Guard427Gate(1) then"),
        source
            .replace("if Guard427Gate() then", "if not Guard427Gate() then")
            .replace("if choice <= 1 then assert", "if choice >= 2 then assert")
            .replace("    Guard427Decision = \"object\"", "    Guard427Decision = false"),
        source.replace(
            "        function Guard427Installed(value) return tag, Guard427Rows, value end\n    end\nend",
            "        function Guard427Installed(value) return tag, Guard427Rows, value end\n    end\n    Guard427After(Guard427Unknown)\nend",
        ).replace(
            "function Guard427Build()",
            "Guard427Unknown = 27\nfunction Guard427After(value)\n    collectgarbage('collect')\n    trace[#trace + 1] = 'after:' .. value .. ':' .. (weak.condition and 'live' or 'dead')\nend\nfunction Guard427Build()",
        ),
    ] {
        check(&workspace, &changed);
    }
}
