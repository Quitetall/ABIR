#[allow(dead_code)]
#[path = "../build.rs"]
mod build_script;

use std::fs;
use std::path::Path;
use std::process::Command;

const TEST_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const TEST_MANIFEST: &str = "[package]\nname = \"abir-python\"\nversion = \"0.0.0\"\n";

#[test]
fn accepts_exact_lowercase_git_revision() {
    assert!(build_script::validate_revision(TEST_REVISION).is_ok());
    assert_eq!(
        build_script::parse_git_revision_output(b"0123456789abcdef0123456789abcdef01234567\n")
            .unwrap(),
        "0123456789abcdef0123456789abcdef01234567"
    );
    assert_eq!(
        build_script::parse_git_revision_output(b"0123456789abcdef0123456789abcdef01234567\r\n")
            .unwrap(),
        "0123456789abcdef0123456789abcdef01234567"
    );
}

#[test]
fn development_build_uses_unmistakable_non_revision_identity() {
    assert_eq!(
        build_script::implementation_revision_from_development_build("1").unwrap(),
        build_script::DEVELOPMENT_REVISION
    );
    for value in ["", "0", "true", TEST_REVISION] {
        assert!(
            build_script::implementation_revision_from_development_build(value).is_err(),
            "accepted malformed development opt-in {value:?}"
        );
    }
}

#[test]
fn rejects_noncanonical_git_revisions() {
    for revision in [
        "",
        "0123456789abcdef0123456789abcdef0123456",
        "0123456789abcdef0123456789abcdef012345678",
        "0123456789ABCDEF0123456789ABCDEF01234567",
        "g123456789abcdef0123456789abcdef01234567",
        " 123456789abcdef0123456789abcdef01234567",
        "0123456789abcdef0123456789abcdef0123456\n",
    ] {
        assert!(
            build_script::validate_revision(revision).is_err(),
            "accepted malformed revision {revision:?}"
        );
    }
}

#[test]
fn rejects_malformed_git_command_output() {
    for output in [
        &b""[..],
        &b"\n"[..],
        &b"0123456789abcdef0123456789abcdef01234567\n\n"[..],
        &b"0123456789abcdef0123456789abcdef01234567 extra\n"[..],
        &b"\xff\xfe\n"[..],
    ] {
        assert!(
            build_script::parse_git_revision_output(output).is_err(),
            "accepted malformed git output {output:?}"
        );
    }
}

#[test]
fn accepts_current_abir_repository_authority() {
    assert!(
        build_script::validate_repository_authority(Path::new(env!("CARGO_MANIFEST_DIR"))).is_ok()
    );
}

#[test]
fn revision_tracking_covers_the_complete_abir_workspace() {
    let paths =
        build_script::tracked_worktree_source_paths(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();

    assert!(paths.contains(&workspace_root.join("Cargo.toml")));
    assert!(paths.contains(&workspace_root.join("crates/abir/src/lib.rs")));
    assert!(paths.contains(&workspace_root.join("crates/abir-python/src/lib.rs")));
    assert!(
        paths.len() > 5,
        "workspace tracking regressed to the abir-python crate only"
    );
}

#[test]
fn rejects_source_archive_nested_in_unrelated_git_checkout() {
    let temporary = tempfile::tempdir().unwrap();
    let outer = temporary.path().join("outer");
    let manifest_dir = outer.join("vendor/ABIR/crates/abir-python");
    fs::create_dir_all(&manifest_dir).unwrap();
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .arg(&outer)
        .status()
        .unwrap();
    assert!(status.success());

    let error = build_script::validate_repository_authority(&manifest_dir).unwrap_err();
    assert!(
        error.contains("does not match ABIR workspace root"),
        "{error}"
    );
}

#[test]
fn clean_git_fallback_resolves_head_and_rejects_dirty_checkout() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("ABIR");
    let manifest_dir = repository.join("crates/abir-python");
    fs::create_dir_all(&manifest_dir).unwrap();
    fs::write(repository.join("Cargo.toml"), "[workspace]\n").unwrap();
    fs::write(manifest_dir.join("Cargo.toml"), TEST_MANIFEST).unwrap();
    run_git(&repository, &["init", "--quiet"]);
    run_git(&repository, &["add", "."]);
    run_git(
        &repository,
        &[
            "-c",
            "user.name=ABIR Test",
            "-c",
            "user.email=abir-test@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );
    let expected = git_stdout(&repository, &["rev-parse", "HEAD"]);

    assert_eq!(
        build_script::implementation_revision_from_git_at(&manifest_dir).unwrap(),
        expected
    );

    fs::write(repository.join(".cargo-ok"), []).unwrap();
    assert_eq!(
        build_script::implementation_revision_from_git_at(&manifest_dir).unwrap(),
        expected,
        "Cargo Git checkout sentinel must not make immutable source dirty"
    );

    fs::write(
        manifest_dir.join("Cargo.toml"),
        format!("{TEST_MANIFEST}# dirty\n"),
    )
    .unwrap();
    let error = build_script::implementation_revision_from_git_at(&manifest_dir).unwrap_err();
    assert!(error.contains("checkout is dirty"), "{error}");

    fs::write(manifest_dir.join("Cargo.toml"), TEST_MANIFEST).unwrap();
    fs::write(repository.join("untracked"), "new\n").unwrap();
    let error = build_script::implementation_revision_from_git_at(&manifest_dir).unwrap_err();
    assert!(error.contains("checkout is dirty"), "{error}");
}

#[test]
fn warmed_cargo_build_rechecks_new_untracked_paths() {
    let temporary = tempfile::tempdir().unwrap();
    let repository = temporary.path().join("ABIR");
    let manifest_dir = repository.join("crates/abir-python");
    fs::create_dir_all(manifest_dir.join("src")).unwrap();
    fs::write(
        repository.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/abir-python\"]\n",
    )
    .unwrap();
    fs::write(repository.join(".gitignore"), "/target\n").unwrap();
    fs::write(
        manifest_dir.join("Cargo.toml"),
        "[package]\nname = \"abir-revision-fixture\"\nversion = \"0.0.0\"\n\
         edition = \"2021\"\nbuild = \"build.rs\"\n",
    )
    .unwrap();
    fs::write(manifest_dir.join("build.rs"), include_str!("../build.rs")).unwrap();
    fs::write(
        manifest_dir.join("src/lib.rs"),
        "pub const REVISION: &str = env!(\"ABIR_EMBEDDED_IMPLEMENTATION_REVISION\");\n",
    )
    .unwrap();
    let lock_status = Command::new(
        std::env::var_os("CARGO").unwrap_or_else(|| std::ffi::OsString::from("cargo")),
    )
    .arg("generate-lockfile")
    .arg("--manifest-path")
    .arg(manifest_dir.join("Cargo.toml"))
    .status()
    .unwrap();
    assert!(lock_status.success(), "fixture lockfile generation failed");
    run_git(&repository, &["init", "--quiet"]);
    run_git(&repository, &["add", "."]);
    run_git(
        &repository,
        &[
            "-c",
            "user.name=ABIR Test",
            "-c",
            "user.email=abir-test@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    );

    assert!(
        run_cargo_build(&manifest_dir).success(),
        "initial clean Cargo build must pass"
    );
    assert!(
        run_cargo_build(&manifest_dir).success(),
        "warmed clean Cargo build must pass"
    );

    fs::write(repository.join("unexpected-source"), "untracked\n").unwrap();
    let output = cargo_build_output(&manifest_dir);
    assert!(
        !output.status.success(),
        "dirty warmed build unexpectedly passed"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("checkout is dirty"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_cargo_build(manifest_dir: &Path) -> std::process::ExitStatus {
    cargo_build_command(manifest_dir).status().unwrap()
}

fn cargo_build_output(manifest_dir: &Path) -> std::process::Output {
    cargo_build_command(manifest_dir).output().unwrap()
}

fn cargo_build_command(manifest_dir: &Path) -> Command {
    let mut command = Command::new(
        std::env::var_os("CARGO").unwrap_or_else(|| std::ffi::OsString::from("cargo")),
    );
    command
        .arg("build")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(manifest_dir.join("Cargo.toml"))
        .env_remove("ABIR_DEVELOPMENT_BUILD");
    command
}

fn run_git(repository: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .status()
        .unwrap();
    assert!(status.success(), "git {arguments:?} failed");
}

fn git_stdout(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {arguments:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
