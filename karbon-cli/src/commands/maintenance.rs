//! `karbon maintenance on|off|status` — flips the application's maintenance flag.
//!
//! The flag is a file whose presence the running application checks; this command
//! only creates or removes it. Nothing is sent to the server, so it works whether
//! or not the application is healthy — which is the point, since maintenance is
//! usually declared precisely when something is not.

use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

/// Same fallback as the framework middleware, so an app that configures nothing
/// still gets a coherent answer instead of a confusing one.
const DEFAULT_FLAG_FILE: &str = "storage/maintenance.flag";

/// Where the flag path came from — it decides whether root may trust it.
#[derive(Debug, PartialEq)]
enum FlagSource {
    /// `MAINTENANCE_FLAG_FILE` in the process environment: set by whoever runs us.
    Environment,
    /// `MAINTENANCE_FLAG_FILE` in this `.env`, which usually belongs to the app user.
    DotEnv(PathBuf),
    /// The framework default, inside the project tree.
    Default,
}

pub fn run(root: &Path, action: &str) -> Result<(), String> {
    let (flag, source) = resolve_flag_path(root)?;

    match action {
        "on" | "enable" => {
            refuse_if_unsafe_for_root(&flag, &source)?;
            enable(&flag)
        }
        "off" | "disable" => {
            refuse_if_unsafe_for_root(&flag, &source)?;
            disable(&flag)
        }
        "status" => {
            status(&flag);
            Ok(())
        }
        other => Err(format!("Unknown action '{other}'. Use: on, off, status")),
    }
}

fn enable(flag: &Path) -> Result<(), String> {
    if flag.exists() {
        println!(
            "\n{} Maintenance mode was already on.\n  {}\n",
            "•".yellow().bold(),
            flag.display().to_string().dimmed()
        );
        return Ok(());
    }

    if let Some(parent) = flag
        .parent()
        .filter(|p| !p.as_os_str().is_empty() && !p.exists())
    {
        return Err(format!(
            "Directory {} does not exist.\n       \
             Create it as root, and keep it out of the web tree:\n         \
             sudo mkdir -p {0} && sudo chmod 0755 {0}",
            parent.display()
        ));
    }

    // `create_new` is O_CREAT|O_EXCL: it never follows a final symlink, dangling or not,
    // and fails if anything is there. `fs::write` followed one, so a link planted at the
    // flag path made `sudo karbon maintenance on` create the link's target as root.
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(flag)
        .map_err(|e| {
            format!(
                "Cannot create {}: {e}\n       \
                 The flag is meant to be root-owned, so this usually needs sudo. If a \
                 symbolic link sits at that path, remove it: it is never followed.",
                flag.display()
            )
        })?;

    println!(
        "\n{} Maintenance mode {}\n  {}\n\n  \
         The application answers 503 with a Retry-After header, and its background\n  \
         jobs stop touching the database. Exempt paths keep serving.\n",
        "✔".green().bold(),
        "ON".green().bold(),
        flag.display().to_string().dimmed()
    );
    Ok(())
}

fn disable(flag: &Path) -> Result<(), String> {
    if !flag.exists() {
        println!(
            "\n{} Maintenance mode was already off.\n  {}\n",
            "•".yellow().bold(),
            flag.display().to_string().dimmed()
        );
        return Ok(());
    }

    fs::remove_file(flag).map_err(|e| {
        format!(
            "Cannot remove {}: {e}\n       \
             The flag is meant to be root-owned, so this usually needs sudo.",
            flag.display()
        )
    })?;

    println!(
        "\n{} Maintenance mode {}\n  {}\n\n  \
         Traffic resumes within a second — the application re-checks the file at\n  \
         most once per second, so no restart is needed.\n",
        "✔".green().bold(),
        "OFF".green().bold(),
        flag.display().to_string().dimmed()
    );
    Ok(())
}

fn status(flag: &Path) {
    if flag.exists() {
        println!(
            "\n{} Maintenance mode is {}\n  {}\n",
            "●".red().bold(),
            "ON".red().bold(),
            flag.display().to_string().dimmed()
        );
    } else {
        println!(
            "\n{} Maintenance mode is {}\n  {}\n",
            "●".green().bold(),
            "OFF".green().bold(),
            flag.display().to_string().dimmed()
        );
    }
}

/// Resolve the flag path the same way the application does: the environment
/// first, then `.env`, then the framework default.
///
/// Reading the app's own configuration is what keeps the two in agreement — a
/// hard-coded path here would silently flip a file nobody reads.
fn resolve_flag_path(root: &Path) -> Result<(PathBuf, FlagSource), String> {
    if let Some(path) = std::env::var("MAINTENANCE_FLAG_FILE")
        .ok()
        .filter(|p| !p.trim().is_empty())
    {
        return Ok((PathBuf::from(path.trim()), FlagSource::Environment));
    }

    let env_path = root.join(".env");
    if env_path.exists() {
        let content = fs::read_to_string(&env_path)
            .map_err(|e| format!("Cannot read {}: {e}", env_path.display()))?;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if let Some(value) = line.strip_prefix("MAINTENANCE_FLAG_FILE=") {
                let value = value.trim().trim_matches('"').trim_matches('\'');
                if !value.is_empty() {
                    return Ok((PathBuf::from(value), FlagSource::DotEnv(env_path)));
                }
            }
        }
    }

    Ok((root.join(DEFAULT_FLAG_FILE), FlagSource::Default))
}

/// Under root, refuse a flag path the app user could have chosen or redirected.
///
/// `karbon deploy` hands the project tree to the app user, so its `.env` and `storage/`
/// are theirs. Trusting them, `sudo karbon maintenance off` would unlink whatever they
/// named (`MAINTENANCE_FLAG_FILE=/etc/shadow`), and `on` would create it. As root, the
/// path must therefore come from the environment or a root-owned `.env`, and its
/// directory must be a real, root-owned directory nobody else can write — the setup the
/// flag was designed for. A non-root run can only harm its own files: no check.
fn refuse_if_unsafe_for_root(flag: &Path, source: &FlagSource) -> Result<(), String> {
    #[cfg(unix)]
    {
        if !running_as_root() {
            return Ok(());
        }
        if let FlagSource::DotEnv(env_path) = source
            && !root_controlled(env_path, false)
        {
            return Err(format!(
                "Refusing to run as root with MAINTENANCE_FLAG_FILE read from {}: that file \
                 is not root-owned (or others can write it), so whoever owns it chooses which \
                 file root creates or deletes.\n       \
                 Pass the path explicitly instead:\n         \
                 sudo env MAINTENANCE_FLAG_FILE=/etc/<app>/maintenance.flag karbon maintenance <on|off>",
                env_path.display()
            ));
        }
        let parent = flag
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        if !root_controlled(parent, true) {
            return Err(format!(
                "Refusing to run as root on {}: its directory is not a root-owned directory \
                 that only root can write, so another user could redirect the flag.\n       \
                 Keep the flag in a root-owned directory, out of the web tree:\n         \
                 sudo mkdir -p /etc/<app> && sudo chmod 0755 /etc/<app>\n       \
                 and point MAINTENANCE_FLAG_FILE at /etc/<app>/maintenance.flag.",
                flag.display()
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = (flag, source);
    Ok(())
}

#[cfg(unix)]
fn running_as_root() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

/// `path` is not a symlink, is a directory when `want_dir`, and only root can write it.
#[cfg(unix)]
fn root_controlled(path: &Path, want_dir: bool) -> bool {
    use std::os::unix::fs::MetadataExt;
    let Ok(meta) = fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink() || (want_dir && !meta.is_dir()) {
        return false;
    }
    is_root_controlled(meta.uid(), meta.mode())
}

/// Owned by root, and neither group nor others may write it.
#[cfg_attr(not(unix), allow(dead_code))]
fn is_root_controlled(uid: u32, mode: u32) -> bool {
    uid == 0 && mode & 0o022 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_file_wins_over_the_default() {
        let dir = std::env::temp_dir().join("karbon-maint-test-env");
        let _ = fs::create_dir_all(&dir);
        fs::write(
            dir.join(".env"),
            "# comment\nDB_HOST=x\nMAINTENANCE_FLAG_FILE=/etc/app/maintenance.flag\n",
        )
        .unwrap();

        let (resolved, source) = resolve_flag_path(&dir).unwrap();
        assert_eq!(resolved, PathBuf::from("/etc/app/maintenance.flag"));
        assert_eq!(source, FlagSource::DotEnv(dir.join(".env")));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_the_framework_default() {
        let dir = std::env::temp_dir().join("karbon-maint-test-empty");
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);

        let (resolved, source) = resolve_flag_path(&dir).unwrap();
        assert_eq!(resolved, dir.join(DEFAULT_FLAG_FILE));
        assert_eq!(source, FlagSource::Default);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn quoted_values_are_unwrapped() {
        let dir = std::env::temp_dir().join("karbon-maint-test-quoted");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join(".env"), "MAINTENANCE_FLAG_FILE=\"/tmp/flag\"\n").unwrap();

        assert_eq!(
            resolve_flag_path(&dir).unwrap().0,
            PathBuf::from("/tmp/flag")
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn on_then_off_is_idempotent() {
        let dir = std::env::temp_dir().join("karbon-maint-test-toggle");
        let _ = fs::create_dir_all(&dir);
        let flag = dir.join("maintenance.flag");
        let _ = fs::remove_file(&flag);

        assert!(enable(&flag).is_ok());
        assert!(flag.exists());
        assert!(enable(&flag).is_ok(), "enabling twice must not fail");

        assert!(disable(&flag).is_ok());
        assert!(!flag.exists());
        assert!(disable(&flag).is_ok(), "disabling twice must not fail");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn only_a_root_owned_path_nobody_else_writes_counts_as_root_controlled() {
        assert!(is_root_controlled(0, 0o040755));
        assert!(is_root_controlled(0, 0o100644));
        assert!(!is_root_controlled(1000, 0o040755), "owned by the app user");
        assert!(!is_root_controlled(0, 0o040775), "group-writable");
        assert!(!is_root_controlled(0, 0o040777), "world-writable");
    }

    /// The exploit: a dangling symlink at the flag path made `on` create its target.
    /// No root needed to show it — `fs::write` followed the link for anyone.
    #[cfg(unix)]
    #[test]
    fn enable_never_follows_a_symlink_at_the_flag_path() {
        let dir = std::env::temp_dir().join(format!("karbon-maint-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("planted-target");
        let flag = dir.join("maintenance.flag");
        std::os::unix::fs::symlink(&target, &flag).unwrap();

        assert!(enable(&flag).is_err());
        assert!(!target.exists(), "the link's target must not be created");

        let _ = fs::remove_dir_all(&dir);
    }

    /// A temp directory belongs to the test user, never to root: as root it is refused,
    /// as anyone else the check does not apply.
    #[cfg(unix)]
    #[test]
    fn root_refuses_a_flag_in_a_directory_another_user_owns() {
        let dir = std::env::temp_dir().join(format!("karbon-maint-owner-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let result = refuse_if_unsafe_for_root(&dir.join("maintenance.flag"), &FlagSource::Default);
        assert_eq!(result.is_err(), running_as_root());
        let _ = fs::remove_dir_all(&dir);
    }
}
