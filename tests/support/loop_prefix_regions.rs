//! 模块前缀循环只在整体帧证书成立时接入后续构造，验证顺序与拒绝边界。
use super::*;

const SOURCE: &str = include_str!("../regress-case/regress_425_global_loop_regions.lua");

fn whole_builder(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

#[test]
fn regressions_425_pairs_prefix_keeps_iterator_slots_gc_and_exceptions() {
    assert_roundtrip_trace(&Workspace::new(), SOURCE, whole_builder);
}

#[test]
fn regressions_425_pairs_prefix_keeps_large_constant_pool_and_lower_captures() {
    let mut prefix = String::new();
    for value in 0..270 {
        writeln!(prefix, "Loop425Load(\"pool425_{value}\")").unwrap();
    }
    let source = SOURCE
        .replace("Loop425Load(\"prefix\")", &prefix)
        .replace("for fail = 1, step do", "for fail = 1, 8 do");
    assert_roundtrip_trace(&Workspace::new(), &source, whole_builder);
}

#[test]
fn regressions_425_pairs_prefix_does_not_certify_other_bodies_or_bindings() {
    let workspace = Workspace::new();
    for source in [
        SOURCE.replace("value * Loop425Factor", "value + Loop425Factor"),
        SOURCE.replace(
            "Loop425First[key] = value * Loop425Factor",
            "marker = value",
        ),
        SOURCE.replace("for key, value in pairs", "for key, value, extra in pairs"),
        SOURCE.replace("in pairs(Loop425First)", "in ipairs(Loop425First)"),
    ] {
        for strip in [true, false] {
            let bytes = compile(&workspace, &source, strip);
            let error =
                decompile(&bytes, options(NamingMode::Simple)).expect_err("unproved loop prefix");
            assert!(
                error.to_string().contains("residual table-set-list"),
                "{error}"
            );
        }
    }
}
