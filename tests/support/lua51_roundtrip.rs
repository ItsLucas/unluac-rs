use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static WORKSPACE_ID: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct Workspace(PathBuf);

impl Workspace {
    pub(crate) fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "unluac-constructor-roundtrip-{}-{}",
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

pub(crate) fn tool(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("lua/build/lua5.1")
        .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

pub(crate) fn with_stdin(command: &mut Command, source: &str) -> Output {
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

pub(crate) fn compile(workspace: &Workspace, source: &str, strip: bool) -> Vec<u8> {
    let path = workspace.0.join("case.luac");
    let mut compiler = Command::new(tool("luac"));
    if strip {
        compiler.arg("-s");
    }
    with_stdin(compiler.arg("-o").arg(&path).arg("-"), source);
    fs::read(path).unwrap()
}
