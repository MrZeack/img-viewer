use std::process::Command;

#[test]
fn version_flag_prints_manifest_version_and_exits() {
    for flag in ["--version", "-V"] {
        let out = Command::new(env!("CARGO_BIN_EXE_img_viewer"))
            .arg(flag)
            .output()
            .expect("binary runs");
        assert!(out.status.success(), "{flag} should exit 0");
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            format!("img-viewer {}", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn help_flag_prints_usage_and_exits() {
    for flag in ["--help", "-h"] {
        let out = Command::new(env!("CARGO_BIN_EXE_img_viewer"))
            .arg(flag)
            .output()
            .expect("binary runs");
        assert!(out.status.success(), "{flag} should exit 0");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(stdout.contains("img-viewer"), "stdout: {stdout}");
        assert!(stdout.contains("--version"), "stdout: {stdout}");
    }
}

#[test]
fn invalid_path_argument_exits_with_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_img_viewer"))
        .arg("/definitely/nonexistent/img_viewer_path_xyz")
        .output()
        .expect("binary runs");
    assert!(!out.status.success(), "missing path must fail");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.trim().is_empty(),
        "expected error message on stderr"
    );
}
