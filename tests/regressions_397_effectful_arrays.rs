//! Exercise strict generation, execution and canonical opcode traces with original fixtures.
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use unluac::decompile::{
    DecompileOptions, GenerateMode, GeneratedChunkKind, NamingMode, decompile,
};
use unluac::parser::RawLiteralConst;
use unluac::transformer::{AccessBase, LowInstr, LoweredProto};

static WORKSPACE_ID: AtomicUsize = AtomicUsize::new(0);

struct Workspace(PathBuf);

impl Workspace {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "unluac-regressions-397-{}-{}",
            std::process::id(),
            WORKSPACE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).expect("create isolated regression workspace");
        Self(path)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove regression workspace");
    }
}

fn tool(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("lua/build/lua5.1")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

fn with_stdin(command: &mut Command, source: &str) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run pinned Lua 5.1 toolchain");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .expect("write Lua source");
    let output = child.wait_with_output().expect("wait for Lua toolchain");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn compile(workspace: &Workspace, source: &str, strip: bool) -> Vec<u8> {
    let path = workspace.0.join("case.luac");
    let mut compiler = Command::new(tool("luac"));
    if strip {
        compiler.arg("-s");
    }
    with_stdin(compiler.arg("-o").arg(&path).arg("-"), source);
    fs::read(path).unwrap()
}

fn source_with_rows(rows: usize) -> String {
    let template = include_str!("regress-case/regress_397_effectful_array_regions.lua");
    let start = template.find("-- region397 rows begin").unwrap();
    let end = template.find("-- region397 rows end").unwrap();
    let mut source = template[..start].to_owned();
    for row in 1..=rows {
        writeln!(
            source,
            "{{Region397Value({row}), \"\" .. object.name .. Region397Suffix({}), 9, (Region397Value({row}))}},",
            10 + row
        )
        .unwrap();
    }
    source.push_str(&template[end..]);
    source
}

fn constructor_instructions(proto: &LoweredProto) -> Option<&[LowInstr]> {
    let start = proto.instrs.iter().position(|instr| {
        matches!(instr, LowInstr::NewTable(table)
            if table.lua51_allocation.is_some_and(|hint| hint.hash_hint == 0))
    })?;
    let end = proto.instrs[start..].iter().position(
        |instr| matches!(instr, LowInstr::SetTable(store) if store.base == AccessBase::Env),
    )? + start;
    Some(&proto.instrs[start..=end])
}

#[test]
fn regressions_397_strict_effectful_arrays_preserve_execution_and_raw_trace() {
    let workspace = Workspace::new();
    for rows in [2, 49, 50, 51, 171] {
        let source = source_with_rows(rows);
        let expected = with_stdin(Command::new(tool("lua")).arg("-"), &source).stdout;
        for strip in [true, false] {
            let bytes = compile(&workspace, &source, strip);
            for mode in [
                NamingMode::Simple,
                NamingMode::DebugLike,
                NamingMode::Heuristic,
            ] {
                let mut options = DecompileOptions::default();
                options.generate.mode = GenerateMode::Strict;
                options.parse.string_encoding = "gbk".parse().unwrap();
                options.naming.mode = mode;
                let result = decompile(&bytes, options.clone())
                    .unwrap_or_else(|error| panic!("rows={rows}, strip={strip}: {error}"));
                let generated = result.state.generated.unwrap();
                assert_eq!(generated.kind, GeneratedChunkKind::Source);
                assert!(!generated.source.contains("unluac error"));
                let output = with_stdin(Command::new(tool("lua")).arg("-"), &generated.source);
                assert_eq!(
                    output.stdout, expected,
                    "rows={rows}, strip={strip}, mode={mode:?}"
                );
                let compiled = compile(&workspace, &generated.source, strip);
                let roundtrip = decompile(&compiled, options).expect("strict roundtrip");
                let before = result.state.lowered.unwrap();
                let after = roundtrip.state.lowered.unwrap();
                let original_regions = before
                    .main
                    .children
                    .iter()
                    .filter_map(|proto| constructor_instructions(proto))
                    .collect::<Vec<_>>();
                let generated_regions = after
                    .main
                    .children
                    .iter()
                    .filter_map(|proto| constructor_instructions(proto))
                    .collect::<Vec<_>>();
                assert_eq!(
                    original_regions.len(),
                    1,
                    "must compare the builder's region"
                );
                // Constants and register writes must match, not merely the final table
                // values or the number of CALLs.
                assert_eq!(
                    original_regions, generated_regions,
                    "rows={rows}, strip={strip}, mode={mode:?}"
                );
                let original = before
                    .main
                    .children
                    .iter()
                    .find(|proto| constructor_instructions(proto).is_some())
                    .unwrap();
                let generated = after
                    .main
                    .children
                    .iter()
                    .find(|proto| constructor_instructions(proto).is_some())
                    .unwrap();
                let original = &original.constants.common.literals;
                let generated = &generated.constants.common.literals;
                assert_eq!(original.len(), generated.len());
                for (left, right) in original.iter().zip(generated) {
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
}

#[test]
fn regressions_397_noncanonical_allocation_gap_and_owner_read_stay_rejected() {
    let workspace = Workspace::new();
    let source = source_with_rows(2);
    let bytes = compile(&workspace, &source, true);
    let mut options = DecompileOptions::default();
    options.generate.mode = GenerateMode::Strict;
    options.parse.string_encoding = "gbk".parse().unwrap();
    let baseline = decompile(&bytes, options.clone()).unwrap();
    let lowered = baseline.state.lowered.unwrap();
    let builder = lowered
        .main
        .children
        .iter()
        .position(|proto| constructor_instructions(proto).is_some())
        .unwrap();
    let proto = &lowered.main.children[builder];
    let raw = baseline.state.raw_chunk.unwrap();
    let raw_proto = &raw.main.common.children[builder];
    let root_pc = proto
        .instrs
        .iter()
        .position(|instr| matches!(instr, LowInstr::NewTable(_)))
        .unwrap();
    let LowInstr::NewTable(root) = proto.instrs[root_pc] else {
        unreachable!();
    };
    let list_pc = proto
        .instrs
        .iter()
        .position(|instr| matches!(instr, LowInstr::SetList(_)))
        .unwrap();
    let read_pc = proto
        .instrs
        .iter()
        .position(|instr| {
            matches!(instr, LowInstr::GetTable(get)
            if matches!(get.base, AccessBase::Reg(_)))
        })
        .unwrap();

    // Lua 5.1 B is bits 23..31 and C is bits 14..22. Use parser origins rather
    // than searching byte strings or assuming the chunk's header/size_t width.
    for (pc, shift, value) in [
        (root_pc, 23, 0),
        (list_pc, 14, 2),
        (read_pc, 23, root.dst.index() as u32),
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
        let error = decompile(&changed, options.clone()).expect_err("proof must fail closed");
        assert!(
            error.to_string().contains("residual table-set-list"),
            "unexpected rejection: {error}"
        );
    }
}
