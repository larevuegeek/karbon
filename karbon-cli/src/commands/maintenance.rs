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

pub fn run(root: &Path, action: &str) -> Result<(), String> {
    let flag = resolve_flag_path(root)?;

    match action {
        "on" | "enable" => enable(&flag),
        "off" | "disable" => disable(&flag),
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

    fs::write(flag, b"").map_err(|e| {
        format!(
            "Cannot create {}: {e}\n       \
             The flag is meant to be root-owned, so this usually needs sudo.",
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
fn resolve_flag_path(root: &Path) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var("MAINTENANCE_FLAG_FILE")
        .ok()
        .filter(|p| !p.trim().is_empty())
    {
        return Ok(PathBuf::from(path.trim()));
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
                    return Ok(PathBuf::from(value));
                }
            }
        }
    }

    Ok(root.join(DEFAULT_FLAG_FILE))
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

        let resolved = resolve_flag_path(&dir).unwrap();
        assert_eq!(resolved, PathBuf::from("/etc/app/maintenance.flag"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_the_framework_default() {
        let dir = std::env::temp_dir().join("karbon-maint-test-empty");
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::create_dir_all(&dir);

        let resolved = resolve_flag_path(&dir).unwrap();
        assert_eq!(resolved, dir.join(DEFAULT_FLAG_FILE));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn quoted_values_are_unwrapped() {
        let dir = std::env::temp_dir().join("karbon-maint-test-quoted");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join(".env"), "MAINTENANCE_FLAG_FILE=\"/tmp/flag\"\n").unwrap();

        assert_eq!(resolve_flag_path(&dir).unwrap(), PathBuf::from("/tmp/flag"));

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
}
