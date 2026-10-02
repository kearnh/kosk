use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    let commit = git_stdout(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let dirty = match git_stdout(&["status", "--porcelain"]) {
        Some(s) => !s.is_empty(),
        None => true,
    };

    println!("cargo:rustc-env=GIT_COMMIT={commit}");
    println!(
        "cargo:rustc-env=GIT_DIRTY={}",
        if dirty { "1" } else { "0" }
    );
}

fn git_stdout(args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.trim().to_string();
    if s.is_empty() && args[0] == "rev-parse" {
        return None;
    }
    Some(s)
}
