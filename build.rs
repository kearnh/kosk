use std::process::Command;

fn main() {
    embed_windows_resources();

    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    println!("cargo:rerun-if-env-changed=KOSK_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=KOSK_BUILD_DIRTY");

    let commit = std::env::var("KOSK_BUILD_COMMIT")
        .ok()
        .or_else(|| jj_stdout(&["log", "-r", "@-", "--no-graph", "-T", "commit_id"]))
        .unwrap_or_else(|| "unknown".to_string());
    let dirty = std::env::var("KOSK_BUILD_DIRTY")
        .ok()
        .or_else(|| jj_stdout(&["log", "-r", "@", "--no-graph", "-T", "if(empty, '0', '1')"]))
        .unwrap_or_else(|| "1".to_string());

    println!("cargo:rustc-env=GIT_COMMIT={commit}");
    println!("cargo:rustc-env=GIT_DIRTY={}", dirty);
}

fn embed_windows_resources() {
    println!("cargo:rerun-if-changed=assets/kosk.ico");
    println!("cargo:rerun-if-changed=Cargo.toml");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    winresource::WindowsResource::new()
        .set_icon("assets/kosk.ico")
        .set("FileDescription", env!("CARGO_PKG_DESCRIPTION"))
        .set("ProductName", "KOSK")
        .set("InternalName", "kosk")
        .set("OriginalFilename", "kosk.exe")
        .set("LegalCopyright", "Copyright (c) 2026 KOSK contributors")
        .compile()
        .expect("Could not compile Windows executable resources");
}

fn jj_stdout(args: &[&str]) -> Option<String> {
    let out = Command::new("jj")
        .arg("--ignore-working-copy")
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim().to_string();
    if s.is_empty() {
        return None;
    }
    Some(s)
}
