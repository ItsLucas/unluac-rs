//! 全局前缀语句保留每次寄存器写入与可观察效果，调用结果写回也须逐项对照。
use super::*;

const SOURCE: &str = include_str!("../regress-case/regress_421_global_value_regions.lua");

fn whole_builder(proto: &LoweredProto) -> Option<&[LowInstr]> {
    Some(&proto.instrs)
}

fn check(workspace: &Workspace, source: &str) {
    assert_roundtrip_trace(workspace, source, whole_builder);
}

#[test]
fn regressions_421_global_values_keep_gc_errors_and_closure_captures() {
    check(&Workspace::new(), SOURCE);
}

#[test]
fn regressions_421_scalar_store_bridges_complete_global_constructors() {
    let source = include_str!("../regress-case/regress_417_module_record_calls.lua").replace(
        "Module417Rows = {",
        "Module417Side = 1\n    Module417Rows = {",
    );
    check(&Workspace::new(), &source);
}

#[test]
fn regressions_421_preceding_closure_keeps_captured_scalar_source_slot() {
    let source = SOURCE
        .replace(
            "function Global421Build(captured)",
            "function Global421Build()",
        )
        .replace(
            "    function Global421Captured(value)",
            "    local captured = 17\n    function Global421Captured(value)",
        )
        .replace("Global421Build(17)", "Global421Build()")
        .replace("pcall(Global421Build, 17)", "pcall(Global421Build)");
    check(&Workspace::new(), &source);
}

#[test]
fn regressions_421_global_values_keep_large_keys_and_numeric_rhs_slots() {
    let workspace = Workspace::new();
    for prefix in [240, 248, 250, 252, 254, 256, 270] {
        let mut loads = String::new();
        for value in 0..prefix {
            writeln!(loads, "Global421Load(\"pool421_{value}\")").unwrap();
        }
        let source = SOURCE
            .replace("Global421Load(\"prefix\")", &loads)
            .replace("4 * Global421Number", "Global421Number * 4")
            .replace("for fail = 1, step do", "for fail = 1, 8 do");
        check(&workspace, &source);
    }
}

#[test]
fn regressions_421_global_arithmetic_keeps_runtime_operand_order() {
    let workspace = Workspace::new();
    for expression in [
        "Global421Number * 4",
        "(Global421Number + 64) * 2",
        "(Global421Number / 2) ^ 2 / 4",
        "(Global421Number % 33 + 1) * 8",
        "320 - Global421Number",
    ] {
        check(
            &workspace,
            &SOURCE.replace("4 * Global421Number", expression),
        );
    }
}

#[test]
fn regressions_421_global_call_result_assignment_keeps_the_frame_and_errors() {
    let source = SOURCE
        .replace("Global421Number = 64", "Global421Number = Global421Unknown()")
        .replace(
            "function Global421Build(captured)",
            "function Global421Unknown()\n    event('number-call')\n    return 64, {}, nil\nend\nfunction Global421Build(captured)",
        );
    check(&Workspace::new(), &source);
}

#[test]
fn regressions_421_global_values_do_not_skip_unproved_locals_or_keys() {
    let workspace = Workspace::new();
    for source in [
        SOURCE.replace(
            "Global421Value = Global421Catalog.group.value",
            "local unknown = Global421Catalog.group.value\n    Global421Value = unknown",
        ),
        SOURCE.replace(
            "Global421Value = Global421Catalog.group.value",
            "Global421Value = Global421Catalog[Global421Key]",
        ),
    ] {
        for strip in [true, false] {
            let bytes = compile(&workspace, &source, strip);
            let error = decompile(&bytes, options(NamingMode::Simple))
                .expect_err("uncertified prefix must retain the residual constructor");
            assert!(
                error.to_string().contains("residual table-set-list"),
                "{error}"
            );
        }
    }
}
