use std::fmt::Write as _;
use std::process::Command;

#[path = "support/lua51_roundtrip.rs"]
mod lua51_roundtrip;
use lua51_roundtrip::{Workspace, compile, tool, with_stdin};

use unluac::decompile::{
    DecompileOptions, GenerateMode, GeneratedChunkKind, NamingMode, decompile,
};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{AccessBase, AccessKey, LowInstr, LoweredProto, ValueOperand};

fn options(mode: NamingMode) -> DecompileOptions {
    let mut options = DecompileOptions::default();
    options.generate.mode = GenerateMode::Strict;
    options.parse.string_encoding = "gbk".parse().unwrap();
    options.naming.mode = mode;
    options
}

fn source_with_records(rows: usize, fields: usize) -> String {
    let template = include_str!("regress-case/regress_398_record_lookup_regions.lua");
    let start = template.find("-- region398 rows begin").unwrap();
    let end = template.find("-- region398 rows end").unwrap();
    let mut source = template[..start].to_owned();
    for _ in 0..rows {
        source.push_str("{alpha = Record398Catalog.group.alpha, missing = Record398Catalog.group.missing, pad = 3");
        for field in 3..fields {
            write!(
                source,
                ", field{field} = Record398Catalog.group.field{field}"
            )
            .unwrap();
        }
        source.push_str("},\n");
    }
    source.push_str(&template[end..]);
    source
}

fn prefix_and_region(proto: &LoweredProto) -> Option<&[LowInstr]> {
    let start = proto
        .instrs
        .iter()
        .position(|instr| matches!(instr, LowInstr::NewTable(_)))?;
    let end = proto.instrs[start..].iter().position(
        |instr| matches!(instr, LowInstr::SetTable(store) if store.base == AccessBase::Env),
    )? + start;
    Some(&proto.instrs[..=end])
}

fn builder(proto: &LoweredProto) -> &LoweredProto {
    let mut builders = proto
        .children
        .iter()
        .filter(|proto| prefix_and_region(proto).is_some());
    let result = builders.next().expect("original builder exists");
    assert!(
        builders.next().is_none(),
        "must select a unique constructor"
    );
    result
}

fn assert_roundtrip(workspace: &Workspace, source: &str) {
    let expected = with_stdin(Command::new(tool("lua")).arg("-"), source).stdout;
    for strip in [true, false] {
        let bytes = compile(workspace, source, strip);
        for mode in [
            NamingMode::Simple,
            NamingMode::DebugLike,
            NamingMode::Heuristic,
        ] {
            let result = decompile(&bytes, options(mode)).expect("strict GBK decompilation");
            let generated = result.state.generated.unwrap();
            assert_eq!(generated.kind, GeneratedChunkKind::Source);
            assert!(!generated.source.contains("unluac error"));
            with_stdin(
                Command::new(tool("luac")).args(["-p", "-"]),
                &generated.source,
            );
            let actual = with_stdin(Command::new(tool("lua")).arg("-"), &generated.source);
            assert_eq!(actual.stdout, expected, "strip={strip}, mode={mode:?}");

            let recompiled = compile(workspace, &generated.source, strip);
            let roundtrip = decompile(&recompiled, options(mode)).expect("strict roundtrip");
            let before = result.state.lowered.unwrap();
            let after = roundtrip.state.lowered.unwrap();
            let before = builder(&before.main);
            let after = builder(&after.main);
            assert_eq!(
                prefix_and_region(before),
                prefix_and_region(after),
                "including ignored-call prefix, all physical writes must match"
            );
            let before = &before.constants.common.literals;
            let after = &after.constants.common.literals;
            assert_eq!(before.len(), after.len());
            for (left, right) in before.iter().zip(after) {
                match (left, right) {
                    (RawLiteralConst::String(left), RawLiteralConst::String(right)) => {
                        assert_eq!(left.bytes, right.bytes)
                    }
                    _ => assert_eq!(left, right),
                }
            }
        }
    }
}

#[test]
fn regressions_398_open_array_tail_stays_outside_the_record_proof() {
    let workspace = Workspace::new();
    let source = format!(
        "function Record398Open() return nil, 7 end\n{}",
        source_with_records(2, 3).replace(
            "-- region398 rows end",
            "Record398Open(),\n-- region398 rows end"
        )
    );
    with_stdin(Command::new(tool("lua")).arg("-"), &source);
    let bytes = compile(&workspace, &source, true);
    let error = decompile(&bytes, options(NamingMode::Simple))
        .expect_err("open results require a separate proof");
    assert!(
        error.to_string().contains("residual table-set-list"),
        "{error}"
    );
}

#[test]
fn regressions_398_lookup_order_and_rounded_record_allocations() {
    let workspace = Workspace::new();
    for (rows, fields) in [(2, 3), (49, 3), (50, 3), (51, 3), (2, 17), (2, 18)] {
        assert_roundtrip(&workspace, &source_with_records(rows, fields));
    }
}

#[test]
fn regressions_399_lookup_scratch_roots_preserve_gc_observations() {
    let workspace = Workspace::new();
    assert_roundtrip(
        &workspace,
        include_str!("regress-case/regress_399_record_lookup_gc.lua"),
    );
}

#[test]
fn regressions_398_noncanonical_records_remain_rejected() {
    let workspace = Workspace::new();
    let source = source_with_records(2, 3);
    let bytes = compile(&workspace, &source, true);
    let baseline = decompile(&bytes, options(NamingMode::Simple)).unwrap();
    let lowered = baseline.state.lowered.unwrap();
    let index = lowered
        .main
        .children
        .iter()
        .position(|proto| prefix_and_region(proto).is_some())
        .unwrap();
    let proto = &lowered.main.children[index];
    let raw = baseline.state.raw_chunk.unwrap();
    let raw_proto = &raw.main.common.children[index];
    let (child_pc, child) = proto
        .instrs
        .iter()
        .enumerate()
        .find_map(|(pc, instr)| match instr {
            LowInstr::NewTable(table)
                if table
                    .lua51_allocation
                    .is_some_and(|hint| hint.hash_hint > 0) =>
            {
                Some((pc, table))
            }
            _ => None,
        })
        .unwrap();
    let (first_store_pc, first_store) = proto
        .instrs
        .iter()
        .enumerate()
        .find_map(|(pc, instr)| match instr {
            LowInstr::SetTable(store) if store.base == AccessBase::Reg(child.dst) => {
                Some((pc, store))
            }
            _ => None,
        })
        .unwrap();
    let (last_store_pc, last_store) = proto
        .instrs
        .iter()
        .enumerate()
        .filter_map(|(pc, instr)| match instr {
            LowInstr::SetTable(store) if store.base == AccessBase::Reg(child.dst) => {
                Some((pc, store))
            }
            _ => None,
        })
        .last()
        .unwrap();
    let AccessKey::Const(first_key) = first_store.key else {
        unreachable!()
    };
    let ValueOperand::Const(number_key) = last_store.value else {
        unreachable!()
    };
    let read_pc = proto.instrs[..first_store_pc]
        .iter()
        .rposition(|instr| {
            matches!(instr, LowInstr::GetTable(get)
            if get.base == AccessBase::Reg(get.dst))
        })
        .unwrap();

    for (pc, shift, value) in [
        (child_pc, 14, 4), // mismatched NEWTABLE hash allocation
        (last_store_pc, 23, 256 + first_key.index() as u32), // duplicate record key
        (last_store_pc, 23, 256 + number_key.index() as u32), // numeric record key
        (read_pc, 23, child.dst.index() as u32), // observe the fresh owner
    ] {
        let origin = raw_proto.common.instructions[pc].origin;
        assert_eq!(origin.span.size, 4);
        let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
        let patched = (word & !(0x1ff << shift)) | (value << shift);
        let encoded = if bytes[6] == 1 {
            patched.to_le_bytes()
        } else {
            patched.to_be_bytes()
        };
        let mut changed = bytes.clone();
        changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
        let error = decompile(&changed, options(NamingMode::Simple))
            .expect_err("noncanonical trace must not produce source");
        assert!(
            error.to_string().contains("residual table-set-list"),
            "{error}"
        );
    }
}

#[test]
fn regressions_412_arithmetic_order_errors_and_physical_writes() {
    let workspace = Workspace::new();
    assert_roundtrip(
        &workspace,
        include_str!("regress-case/regress_412_record_arithmetic.lua"),
    );
    for rows in [49, 50, 51] {
        let source = source_with_records(rows, 3).replace(
            "Record398Catalog.group.alpha",
            "2 * Record398Catalog.group.alpha * 3",
        );
        assert_roundtrip(&workspace, &source);
    }
    let source = include_str!("regress-case/regress_399_record_lookup_gc.lua")
        .replace(
            "Record399Catalog.group.a",
            "2 * Record399Catalog.group.a * 3",
        )
        .replace("Record399Rows[1].a == 1", "Record399Rows[1].a == 6");
    assert_roundtrip(&workspace, &source);
}

#[test]
fn regressions_412_noncanonical_arithmetic_remains_rejected() {
    let workspace = Workspace::new();
    let source = source_with_records(2, 3).replace(
        "Record398Catalog.group.alpha",
        "2 * Record398Catalog.group.alpha * 3",
    );
    let bytes = compile(&workspace, &source, true);
    let baseline = decompile(&bytes, options(NamingMode::Simple)).unwrap();
    let lowered = baseline.state.lowered.unwrap();
    let index = lowered
        .main
        .children
        .iter()
        .position(|proto| prefix_and_region(proto).is_some())
        .unwrap();
    let proto = &lowered.main.children[index];
    let (pc, binary) = proto
        .instrs
        .iter()
        .enumerate()
        .find_map(|(pc, instr)| {
            if let LowInstr::BinaryOp(binary) = instr {
                Some((pc, binary))
            } else {
                None
            }
        })
        .unwrap();
    let origin = baseline.state.raw_chunk.unwrap().main.common.children[index]
        .common
        .instructions[pc]
        .origin;
    let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
    for (shift, width, value) in [
        (23, 9, binary.dst.index() as u32), // duplicate effectful scratch operand
        (23, 9, 0),                         // read the fresh parent table
        (6, 8, binary.dst.index() as u32 + 1), // write a different scratch home
    ] {
        let patched = (word & !(((1 << width) - 1) << shift)) | (value << shift);
        let encoded = if bytes[6] == 1 {
            patched.to_le_bytes()
        } else {
            patched.to_be_bytes()
        };
        let mut changed = bytes.clone();
        changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
        let error = decompile(&changed, options(NamingMode::Simple))
            .expect_err("noncanonical arithmetic must stay rejected");
        assert!(
            error.to_string().contains("residual table-set-list"),
            "{error}"
        );
    }
}

#[test]
fn regressions_413_captured_root_identity_and_arithmetic_stack() {
    let workspace = Workspace::new();
    assert_roundtrip(
        &workspace,
        include_str!("regress-case/regress_413_captured_record_array.lua"),
    );
}

#[test]
fn regressions_413_captured_lookup_array_flush_boundaries() {
    let workspace = Workspace::new();
    for rows in [2, 49, 50, 51] {
        let source = source_with_records(rows, 3)
            .replace("Record398Rows = {", "local rows = {")
            .replace("-- region398 rows end\n    }", "-- region398 rows end\n    }\n    function Record413Read() return rows end\n    Record398Rows = rows");
        assert_roundtrip(&workspace, &source);
    }
}

#[test]
fn regressions_413_unproved_prefix_and_branch_stay_rejected() {
    let workspace = Workspace::new();
    let source = include_str!("regress-case/regress_413_captured_record_array.lua");
    for changed in [
        source.replace("function Record413Read() return rows end",
            "function Record413Before() return 17 end\n rows = nil\n function Record413Read() return rows end"),
        source.replace(
            "function Record413Build()",
            "function Record413Build()\n local old = Record413Catalog\n",
        ),
        source
            .replace(
                "local rows = {",
                "if Record413Catalog then\n local rows = {",
            )
            .replace(
                "function Record413Replace(value) rows = value end",
                "function Record413Replace(value) rows = value end\n end",
            ),
    ] {
        let bytes = compile(&workspace, &changed, true);
        let error = decompile(&bytes, options(NamingMode::Simple))
            .expect_err("unproved entry frame or branch must not be recovered");
        assert!(
            error.to_string().contains("residual table-set-list"),
            "{error}"
        );
    }
}

#[test]
fn regressions_413_reordered_or_misplaced_scratch_stays_rejected() {
    let workspace = Workspace::new();
    let bytes = compile(
        &workspace,
        include_str!("regress-case/regress_413_captured_record_array.lua"),
        true,
    );
    let baseline = decompile(&bytes, options(NamingMode::Simple)).unwrap();
    let lowered = baseline.state.lowered.unwrap();
    let index = lowered
        .main
        .children
        .iter()
        .position(|proto| prefix_and_region(proto).is_some())
        .unwrap();
    let proto = &lowered.main.children[index];
    let (pc, binary) = proto
        .instrs
        .iter()
        .enumerate()
        .find_map(|(pc, instr)| match instr {
            LowInstr::BinaryOp(binary)
                if matches!(
                    (binary.lhs, binary.rhs),
                    (ValueOperand::Reg(_), ValueOperand::Reg(_))
                ) =>
            {
                Some((pc, binary))
            }
            _ => None,
        })
        .unwrap();
    let ValueOperand::Reg(right) = binary.rhs else {
        unreachable!()
    };
    let origin = baseline.state.raw_chunk.unwrap().main.common.children[index]
        .common
        .instructions[pc]
        .origin;
    let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
    for patched in [
        // Reverse the two operands; arithmetic/metamethod order is observable.
        (word & !((0x1ff << 23) | (0x1ff << 14)))
            | ((right.index() as u32) << 23)
            | ((binary.dst.index() as u32) << 14),
        // Keep the second temporary instead of reducing to the first stack home.
        (word & !(0xff << 6)) | ((right.index() as u32) << 6),
    ] {
        let encoded = if bytes[6] == 1 {
            patched.to_le_bytes()
        } else {
            patched.to_be_bytes()
        };
        let mut changed = bytes.clone();
        changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
        let error = decompile(&changed, options(NamingMode::Simple))
            .expect_err("noncanonical stack reduction must stay rejected");
        assert!(
            error.to_string().contains("residual table-set-list"),
            "{error}"
        );
    }
}

#[test]
fn regressions_413_binary_trees_preserve_stack_and_association() {
    let workspace = Workspace::new();
    let source = include_str!("regress-case/regress_412_record_arithmetic.lua")
        .replace(
            "2 * Record412Catalog.a * 3",
            "(Record412Catalog.a + Record412Catalog.b) * Record412Catalog.c",
        )
        .replace(
            "(Record412Catalog.b + 4) - 5",
            "Record412Catalog.a - (Record412Catalog.b + Record412Catalog.c)",
        )
        .replace(
            "6 / (Record412Catalog.c % 7)",
            "Record412Catalog.a / (Record412Catalog.b % Record412Catalog.c)",
        )
        .replace(
            "(Record412Catalog.d ^ 2) ^ 3",
            "Record412Catalog.a ^ (Record412Catalog.b ^ Record412Catalog.c)",
        )
        .replace(
            "8 - (9 + Record412Catalog.e)",
            "(Record412Catalog.a - Record412Catalog.b) - Record412Catalog.c",
        )
        .replace(
            "2 ^ (Record412Catalog.f / 3)",
            "(Record412Catalog.a ^ Record412Catalog.b) / Record412Catalog.c",
        );
    assert_roundtrip(&workspace, &source);
}

#[test]
#[ignore = "requires a newline-delimited manifest of local game archives"]
fn archive_captured_constructor_roundtrip() {
    let paths = std::env::var("UNLUAC_CAPTURED_ARCHIVE_MANIFEST").expect("archive manifest path");
    let workspace = Workspace::new();
    let mut instructions = 0;
    let mut chunks = 0;
    for path in std::fs::read_to_string(paths).unwrap().lines() {
        let bytes = std::fs::read(path).unwrap();
        let result = decompile(&bytes, options(NamingMode::Simple)).unwrap();
        let source = result.state.generated.unwrap().source;
        // Compile only. Never execute corpus code.
        let recompiled = compile(&workspace, &source, true);
        let mut utf8 = options(NamingMode::Simple);
        utf8.parse.string_encoding = "utf-8".parse().unwrap();
        let roundtrip = decompile(&recompiled, utf8).unwrap();
        let before = result.state.lowered.unwrap();
        let after = roundtrip.state.lowered.unwrap();
        // The entire root includes all closure instructions and capture descriptors.
        assert_eq!(before.main.instrs, after.main.instrs, "{path}");
        instructions += before.main.instrs.len();
        chunks += 1;
        let left = &before.main.constants.common.literals;
        let right = &after.main.constants.common.literals;
        assert_eq!(left.len(), right.len());
        for (a, b) in left.iter().zip(right) {
            match (a, b) {
                (RawLiteralConst::String(a), RawLiteralConst::String(b)) => {
                    assert_eq!(
                        encoding_rs::GBK.decode(&a.bytes).0,
                        encoding_rs::UTF_8.decode(&b.bytes).0
                    );
                }
                _ => assert_eq!(a, b),
            }
        }
    }
    assert!(chunks > 0);
    println!("{chunks} roots, {instructions} instructions including capture descriptors matched");
}

#[test]
fn regressions_414_open_tail_width_holes_and_allocation() {
    let workspace = Workspace::new();
    let template = include_str!("regress-case/regress_414_captured_open_tail.lua");
    for fixed in [0, 1, 49, 50, 51, 100] {
        let source = template
            .replace(
                "-- region414 fixed begin\n        Open414Make(1),",
                &format!(
                    "-- region414 fixed begin\n {}",
                    "Open414Make(1),".repeat(fixed)
                ),
            )
            .replace("rows[1 + i]", &format!("rows[{fixed} + i]"))
            .replace("rows[count + 2]", &format!("rows[count + {}]", fixed + 1));
        let source = if fixed == 0 {
            source
                .replace(" and rows[1] == \"fixed\"", "")
                .replace("for fail = 1, 2 do", "for fail = 2, 2 do")
        } else {
            source
        };
        assert_roundtrip(&workspace, &source);
    }
}

#[test]
fn regressions_414_unproved_open_protocols_remain_rejected() {
    let workspace = Workspace::new();
    let source = include_str!("regress-case/regress_414_captured_open_tail.lua");
    for changed in [
        source.replace("return tag, rows", "return 17, rows"),
        source.replace(
            "function Open414Read() return tag, rows end",
            "function Open414Read() return tag, rows end\n Open414Make(3)",
        ),
        source.replace("Open414Make(2)", "Open414Make(Open414Make(2))"),
    ] {
        let bytes = compile(&workspace, &changed, true);
        let error = decompile(&bytes, options(NamingMode::Simple))
            .expect_err("unproved prefix, suffix or open arguments must stay rejected");
        assert!(
            error.to_string().contains("residual table-set-list"),
            "{error}"
        );
    }
    let bytes = compile(&workspace, source, true);
    let baseline = decompile(&bytes, options(NamingMode::Simple)).unwrap();
    let lowered = baseline.state.lowered.unwrap();
    let index = lowered
        .main
        .children
        .iter()
        .position(|proto| prefix_and_region(proto).is_some())
        .unwrap();
    let proto = &lowered.main.children[index];
    let seed = proto
        .instrs
        .iter()
        .position(|instr| matches!(instr, LowInstr::NewTable(_)))
        .unwrap();
    let flush = proto
        .instrs
        .iter()
        .position(|instr| matches!(instr, LowInstr::SetList(_)))
        .unwrap();
    let raw = baseline.state.raw_chunk.unwrap();
    for (pc, shift, value) in [(seed, 23, 2), (flush, 14, 2), (flush, 23, 1)] {
        let origin = raw.main.common.children[index].common.instructions[pc].origin;
        let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
        let patched = (word & !(0x1ff << shift)) | (value << shift);
        let encoded = if bytes[6] == 1 {
            patched.to_le_bytes()
        } else {
            patched.to_be_bytes()
        };
        let mut changed = bytes.clone();
        changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
        let error = decompile(&changed, options(NamingMode::Simple))
            .expect_err("wrong allocation, batch or fixed-width consumer must stay rejected");
        assert!(
            error.to_string().contains("residual table-set-list"),
            "{error}"
        );
    }
}

fn source_with_large_constant_pool(prefix: usize) -> String {
    let mut calls = "Record398Load(3)\n".to_owned();
    for n in 0..prefix {
        writeln!(calls, "Record398Load(\"pool415_{n}\")").unwrap();
    }
    source_with_records(2, 3).replace(
        "Record398Load(\"first\")\n    Record398Load(\"second\")",
        &calls,
    )
}

#[test]
fn regressions_415_large_record_keys_values_and_lookup_keys() {
    let workspace = Workspace::new();
    for prefix in [240, 248, 250, 252, 254, 256, 270] {
        let source = source_with_large_constant_pool(prefix);
        assert_roundtrip(&workspace, &source);
    }
}

#[test]
fn regressions_415_threshold_exceptions_and_parameter_lookups() {
    let workspace = Workspace::new();
    let source = include_str!("regress-case/regress_415_large_record_keys.lua");
    assert_roundtrip(&workspace, source);
    let source = source_with_large_constant_pool(270)
        .replace(
            "function Record398Build()",
            "function Record398Build(catalog)",
        )
        .replace("Record398Catalog.group", "catalog.group")
        .replace("Record398Build()", "Record398Build(Record398Catalog)");
    assert_roundtrip(&workspace, &source);
}

#[test]
fn regressions_415_noncanonical_key_registers_stay_rejected() {
    let workspace = Workspace::new();
    let bytes = compile(&workspace, &source_with_large_constant_pool(250), true);
    let baseline = decompile(&bytes, options(NamingMode::Simple)).unwrap();
    let lowered = baseline.state.lowered.unwrap();
    let index = lowered
        .main
        .children
        .iter()
        .position(|proto| prefix_and_region(proto).is_some())
        .unwrap();
    let proto = &lowered.main.children[index];
    let (pc, store) = proto
        .instrs
        .iter()
        .enumerate()
        .find_map(|(pc, instr)| match instr {
            LowInstr::SetTable(store) if matches!(store.key, AccessKey::Reg(_)) => {
                Some((pc, store))
            }
            _ => None,
        })
        .unwrap();
    let AccessKey::Reg(key) = store.key else {
        unreachable!()
    };
    let load_pc = proto.instrs[..pc]
        .iter()
        .rposition(|instr| {
            matches!(instr,
        LowInstr::LoadConst(load) if load.dst == key)
        })
        .unwrap();
    let raw = baseline.state.raw_chunk.unwrap();
    for (pc, shift, mask, value) in [
        (pc, 23, 0x1ff, key.index() as u32 + 1), // wrong key register
        (load_pc, 14, 0x3ffff, 2),               // an RK-range string must not have a scratch LOADK
    ] {
        let origin = raw.main.common.children[index].common.instructions[pc].origin;
        let word = u32::try_from(origin.raw_word.unwrap()).unwrap();
        let patched = (word & !(mask << shift)) | (value << shift);
        let encoded = if bytes[6] == 1 {
            patched.to_le_bytes()
        } else {
            patched.to_be_bytes()
        };
        let mut changed = bytes.clone();
        changed[origin.span.offset..origin.span.offset + 4].copy_from_slice(&encoded);
        let error = decompile(&changed, options(NamingMode::Simple))
            .expect_err("noncanonical key materialization must stay rejected");
        assert!(
            error.to_string().contains("residual table-set-list"),
            "{error}"
        );
    }
}

#[test]
fn regressions_415_large_keys_preserve_gc_scratch_roots() {
    let workspace = Workspace::new();
    let mut calls = String::new();
    for n in 0..270 {
        writeln!(calls, "Record399Load(\"pool415_{n}\")").unwrap();
    }
    let source = include_str!("regress-case/regress_399_record_lookup_gc.lua")
        .replace("    Record399Load()", &calls);
    assert_roundtrip(&workspace, &source);
}
