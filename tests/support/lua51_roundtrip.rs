//! 为端到端 Lua 回归隔离临时产物；并发执行器可能处于独立 PID 命名空间。
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static WORKSPACE_ID: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct Workspace(PathBuf);

impl Workspace {
    pub(crate) fn new() -> Self {
        loop {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock after Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "unluac-constructor-roundtrip-{}-{}-{nonce}",
                std::process::id(),
                WORKSPACE_ID.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create isolated regression workspace: {error}"),
            }
        }
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
