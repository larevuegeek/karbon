use colored::Colorize;
use std::collections::HashMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Spawn a child process with inherited stdio
pub fn spawn_command(
    cmd: &str,
    args: &[String],
    dir: &Path,
    label: &str,
) -> Result<Child, String> {
    spawn_command_with_env(cmd, args, dir, label, &HashMap::new())
}

/// Spawn a child process with inherited stdio and extra env vars
pub fn spawn_command_with_env(
    cmd: &str,
    args: &[String],
    dir: &Path,
    label: &str,
    env: &HashMap<String, String>,
) -> Result<Child, String> {
    let mut command = if cfg!(windows) && !cmd.ends_with(".exe") && !cmd.contains('/') && !cmd.contains('\\') {
        let mut c = Command::new("cmd");
        c.args(["/C", cmd]);
        c
    } else {
        Command::new(cmd)
    };

    command
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .envs(env);

    command.spawn().map_err(|e| format!("Failed to start {label} (`{cmd}`): {e}"))
}

/// Wait for any child to exit, then kill the rest.
/// Also handles Ctrl+C to gracefully stop all children.
pub fn wait_for_any(children: &mut Vec<Child>) {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .ok();

    loop {
        if !running.load(Ordering::SeqCst) {
            println!("\n  {} Shutting down...", "■".red());
            break;
        }

        for child in children.iter_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        eprintln!(
                            "  {} A process exited with code {:?}",
                            "✗".red(),
                            status.code()
                        );
                    }
                    running.store(false, Ordering::SeqCst);
                    break;
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }

        if !running.load(Ordering::SeqCst) {
            break;
        }

        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    for child in children.iter_mut() {
        let _ = child.kill();
        let _ = child.wait();
    }

    println!("  {} Stopped.\n", "✓".green());
}
