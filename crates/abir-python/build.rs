use std::env::{self, VarError};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const REVISION_ENV: &str = "ABIR_IMPLEMENTATION_REVISION";

fn main() {
    println!("cargo:rerun-if-env-changed={REVISION_ENV}");

    let revision = implementation_revision()
        .unwrap_or_else(|error| panic!("cannot establish ABIR implementation revision: {error}"));
    println!("cargo:rustc-env=ABIR_EMBEDDED_IMPLEMENTATION_REVISION={revision}");
}

fn implementation_revision() -> Result<String, String> {
    match env::var(REVISION_ENV) {
        Ok(revision) => implementation_revision_from_override(&revision),
        Err(VarError::NotUnicode(_)) => Err(format!("{REVISION_ENV} must be valid UTF-8")),
        Err(VarError::NotPresent) => implementation_revision_from_git(),
    }
}

pub(crate) fn implementation_revision_from_override(revision: &str) -> Result<String, String> {
    validate_revision(revision)
        .map(|()| revision.to_owned())
        .map_err(|error| format!("{REVISION_ENV} {error}"))
}

fn implementation_revision_from_git() -> Result<String, String> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR")
        .map_err(|error| format!("CARGO_MANIFEST_DIR is unavailable: {error}"))?;
    implementation_revision_from_git_at(Path::new(&manifest_dir))
}

pub(crate) fn implementation_revision_from_git_at(manifest_dir: &Path) -> Result<String, String> {
    validate_repository_authority(manifest_dir)?;
    validate_clean_repository(manifest_dir)?;
    track_git_metadata(manifest_dir)?;
    track_worktree_sources(manifest_dir)?;
    let output = git(manifest_dir, &["rev-parse", "HEAD"])?;
    if !output.status.success() {
        return Err(format!(
            "`git rev-parse HEAD` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    parse_git_revision_output(&output.stdout)
}

pub(crate) fn validate_clean_repository(repository: &Path) -> Result<(), String> {
    let output = git(
        repository,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=all",
            "--ignore-submodules=none",
        ],
    )?;
    if !output.status.success() {
        return Err(format!(
            "`git status --porcelain=v1` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    if output.stdout.is_empty() {
        return Ok(());
    }

    let status = String::from_utf8_lossy(&output.stdout);
    let first_change = status.lines().next().unwrap_or("<unparseable change>");
    Err(format!(
        "ABIR source checkout is dirty (first change: {first_change}); commit exact sources or \
         set {REVISION_ENV} explicitly for controlled development or source-archive builds"
    ))
}

pub(crate) fn validate_repository_authority(manifest_dir: &Path) -> Result<(), String> {
    let expected_root = manifest_dir
        .ancestors()
        .nth(2)
        .ok_or_else(|| "CARGO_MANIFEST_DIR has no ABIR workspace root".to_owned())?
        .canonicalize()
        .map_err(|error| format!("cannot resolve ABIR workspace root: {error}"))?;
    let output = git(manifest_dir, &["rev-parse", "--show-toplevel"])?;
    if !output.status.success() {
        return Err(format!(
            "`git rev-parse --show-toplevel` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let actual_root = PathBuf::from(parse_single_line(
        &output.stdout,
        "`git rev-parse --show-toplevel`",
    )?)
    .canonicalize()
    .map_err(|error| format!("cannot resolve Git repository root: {error}"))?;
    if actual_root != expected_root {
        return Err(format!(
            "Git repository root {} does not match ABIR workspace root {}",
            actual_root.display(),
            expected_root.display()
        ));
    }

    let output = git(
        manifest_dir,
        &["cat-file", "-e", "HEAD:crates/abir-python/Cargo.toml"],
    )?;
    if !output.status.success() {
        return Err(
            "Git HEAD does not contain crates/abir-python/Cargo.toml; set \
             ABIR_IMPLEMENTATION_REVISION for source-archive builds"
                .to_owned(),
        );
    }
    Ok(())
}

fn track_git_metadata(repository: &Path) -> Result<(), String> {
    track_git_path(repository, "HEAD")?;
    track_git_path(repository, "index")?;
    track_git_path(repository, "packed-refs")?;

    let output = git(repository, &["symbolic-ref", "-q", "HEAD"])?;
    match output.status.code() {
        Some(0) => {
            let reference = parse_single_line(&output.stdout, "`git symbolic-ref -q HEAD`")?;
            track_git_path(repository, &reference)?;
        }
        Some(1) => {}
        _ => {
            return Err(format!(
                "`git symbolic-ref -q HEAD` failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }
    Ok(())
}

fn track_worktree_sources(repository: &Path) -> Result<(), String> {
    let output = git(repository, &["ls-files", "-z", "--full-name"])?;
    if !output.status.success() {
        return Err(format!(
            "`git ls-files -z --full-name` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let workspace_root = repository
        .ancestors()
        .nth(2)
        .ok_or_else(|| "CARGO_MANIFEST_DIR has no ABIR workspace root".to_owned())?;
    for path in output.stdout.split(|byte| *byte == 0) {
        if path.is_empty() {
            continue;
        }
        let path = std::str::from_utf8(path)
            .map_err(|_| "`git ls-files -z --full-name` returned a non-UTF-8 path".to_owned())?;
        if path.contains(['\r', '\n']) {
            return Err("Git-tracked source path contains a line break".to_owned());
        }
        println!(
            "cargo:rerun-if-changed={}",
            workspace_root.join(path).display()
        );
    }
    Ok(())
}

fn track_git_path(repository: &Path, name: &str) -> Result<(), String> {
    let output = git(repository, &["rev-parse", "--git-path", name])?;
    if !output.status.success() {
        return Err(format!(
            "`git rev-parse --git-path {name}` failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let path = parse_single_line(
        &output.stdout,
        &format!("`git rev-parse --git-path {name}`"),
    )?;
    let path = PathBuf::from(path);
    let path = if path.is_absolute() {
        path
    } else {
        repository.join(path)
    };
    println!("cargo:rerun-if-changed={}", path.display());
    Ok(())
}

fn git(repository: &Path, arguments: &[&str]) -> Result<Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .output()
        .map_err(|error| format!("cannot execute git: {error}"))
}

pub(crate) fn parse_git_revision_output(output: &[u8]) -> Result<String, String> {
    let revision = parse_single_line(output, "`git rev-parse HEAD`")?;
    validate_revision(&revision).map_err(|error| format!("`git rev-parse HEAD` output {error}"))?;
    Ok(revision)
}

fn parse_single_line(output: &[u8], command: &str) -> Result<String, String> {
    let output =
        std::str::from_utf8(output).map_err(|_| format!("{command} returned non-UTF-8 output"))?;
    let line = output
        .strip_suffix("\r\n")
        .or_else(|| output.strip_suffix('\n'))
        .unwrap_or(output);
    if line.is_empty() || line.contains(['\r', '\n']) {
        return Err(format!(
            "{command} did not return exactly one non-empty line"
        ));
    }
    Ok(line.to_owned())
}

pub(crate) fn validate_revision(revision: &str) -> Result<(), String> {
    if revision.len() != 40 {
        return Err("must contain exactly 40 lowercase hexadecimal characters".to_owned());
    }
    if !revision
        .bytes()
        .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err("must contain exactly 40 lowercase hexadecimal characters".to_owned());
    }
    Ok(())
}
