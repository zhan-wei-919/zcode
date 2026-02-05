use std::process::Command;

use tempfile::tempdir;

fn run(cmd: &mut Command) -> (i32, String, String) {
    let out = cmd.output().expect("spawn command");
    let status = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    (status, stdout, stderr)
}

#[test]
fn install_and_uninstall_work_on_custom_bin_dir_without_touching_path() {
    let repo = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let install_sh = repo.join("install.sh");
    let uninstall_sh = repo.join("uninstall.sh");

    // Fake HOME so scripts never touch the real user dir.
    let home = tempdir().unwrap();

    let bin_dir = tempdir().unwrap();
    let other_bin = tempdir().unwrap();

    // A different zcode elsewhere on PATH (must not be deleted by uninstall).
    let other_zcode = other_bin.path().join("zcode");
    std::fs::write(&other_zcode, "do-not-delete\n").unwrap();

    // Dummy binary to install.
    let dummy = tempdir().unwrap();
    let dummy_bin = dummy.path().join("zcode-dummy");
    std::fs::write(&dummy_bin, "hello-from-dummy\n").unwrap();

    // Install into custom bin dir.
    let mut install = Command::new("bash");
    install
        .arg(&install_sh)
        .arg("--bin-dir")
        .arg(bin_dir.path())
        .arg("--binary")
        .arg(&dummy_bin)
        .arg("--no-build")
        .arg("--force")
        .env("HOME", home.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                other_bin.path().display(),
                std::env::var("PATH").unwrap()
            ),
        );
    let (code, _stdout, stderr) = run(&mut install);
    assert_eq!(code, 0, "install failed: {stderr}");

    let installed = bin_dir.path().join("zcode");
    assert!(installed.exists());
    assert_eq!(
        std::fs::read_to_string(&installed).unwrap(),
        "hello-from-dummy\n"
    );

    // Installing again without --force should fail in non-interactive mode.
    let mut install_again = Command::new("bash");
    install_again
        .arg(&install_sh)
        .arg("--bin-dir")
        .arg(bin_dir.path())
        .arg("--binary")
        .arg(&dummy_bin)
        .arg("--no-build")
        .env("HOME", home.path());
    let (code, _stdout, _stderr) = run(&mut install_again);
    assert_ne!(code, 0);

    // Uninstall should only remove the managed path.
    let mut uninstall = Command::new("bash");
    uninstall
        .arg(&uninstall_sh)
        .arg("--bin-dir")
        .arg(bin_dir.path())
        .arg("--force")
        .env("HOME", home.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                other_bin.path().display(),
                std::env::var("PATH").unwrap()
            ),
        );
    let (code, _stdout, stderr) = run(&mut uninstall);
    assert_eq!(code, 0, "uninstall failed: {stderr}");

    assert!(!installed.exists());
    assert!(other_zcode.exists());
    assert_eq!(
        std::fs::read_to_string(&other_zcode).unwrap(),
        "do-not-delete\n"
    );
}
