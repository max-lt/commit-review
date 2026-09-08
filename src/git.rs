//! Runs git in the current directory.

use std::process::Command;

/// Trimmed stdout of a git command that must succeed.
pub fn run(args: &[&str]) -> Result<String, String> {
    Ok(output(args, &[0])?.trim_end().to_string())
}

/// Raw stdout of a `git diff`, which exits 1 with `--no-index` when the
/// files differ.
pub fn diff(args: &[&str]) -> Result<String, String> {
    output(args, &[0, 1])
}

fn output(args: &[&str], ok_codes: &[i32]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .output()
        .map_err(|e| format!("git not found: {e}"))?;
    if !out.status.code().is_some_and(|c| ok_codes.contains(&c)) {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
