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
