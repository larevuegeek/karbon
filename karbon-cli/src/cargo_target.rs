//! Where cargo writes its build output.
//!
//! `<project>/target` for a lone crate, but the workspace root's `target/` when the backend is
//! a workspace member, and whatever `CARGO_TARGET_DIR` or a `.cargo/config.toml` says. Cargo
//! knows: we ask it (`cargo metadata`) instead of assuming `<project>/target`, which left
//! `karbon dev` / `serve` / `deploy` looking for the binary in the wrong place in a workspace.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

/// Cargo's target directory for the project at `root`; `root/target` when cargo cannot tell
/// (no `Cargo.toml`, cargo missing…), which is what Karbon always assumed before.
pub fn target_dir(root: &Path) -> PathBuf {
    query_cargo(root).unwrap_or_else(|| root.join("target"))
}

/// Path of a built binary: `<target>/<profile>/<package>`, `.exe` on Windows when asked.
pub fn binary_path(root: &Path, profile: &str, package: &str, windows_exe: bool) -> PathBuf {
    let name = if windows_exe && cfg!(windows) {
        format!("{package}.exe")
    } else {
        package.to_string()
    };

    target_dir(root).join(profile).join(name)
}

fn query_cargo(root: &Path) -> Option<PathBuf> {
    // `--no-deps`: no dependency resolution, so no network and no lockfile update.
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root)
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    parse_target_directory(&output.stdout)
}

fn parse_target_directory(json: &[u8]) -> Option<PathBuf> {
    #[derive(Deserialize)]
    struct Metadata {
        target_directory: PathBuf,
    }

    serde_json::from_slice::<Metadata>(json)
        .ok()
        .map(|metadata| metadata.target_directory)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_target_directory_from_cargo_metadata() {
        let json = br#"{"packages":[],"target_directory":"/srv/app/target","workspace_root":"/srv/app","version":1}"#;

        assert_eq!(
            parse_target_directory(json),
            Some(PathBuf::from("/srv/app/target"))
        );
        assert_eq!(parse_target_directory(b"not json"), None);
    }

    /// The CLI is itself a workspace member: cargo answers the workspace root's `target/`,
    /// not `karbon-cli/target` — the case that used to break.
    #[test]
    fn a_workspace_member_gets_the_workspace_target() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let expected = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| crate_dir.parent().unwrap().join("target"));

        assert_eq!(
            target_dir(crate_dir).canonicalize().unwrap(),
            expected.canonicalize().unwrap()
        );
    }

    #[test]
    fn without_a_cargo_project_the_previous_default_applies() {
        let dir = std::env::temp_dir().join(format!("karbon-no-cargo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        assert_eq!(target_dir(&dir), dir.join("target"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn binary_paths_follow_the_profile_and_platform() {
        let dir = std::env::temp_dir().join(format!("karbon-bin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let exe = if cfg!(windows) { "app.exe" } else { "app" };

        assert_eq!(
            binary_path(&dir, "debug", "app", true),
            dir.join("target/debug").join(exe)
        );
        assert_eq!(
            binary_path(&dir, "release", "app", false),
            dir.join("target/release/app")
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
