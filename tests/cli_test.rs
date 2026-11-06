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
