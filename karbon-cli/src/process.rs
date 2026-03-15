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
    let mut command = Command::new(cmd);

    command
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .envs(env);

    let child = command
        .spawn()
        .map_err(|e| format!("Failed to start {label} (`{cmd}`): {e}"))?;

    // On Windows, assign to a shared Job Object so all children
    // get killed when the parent (karbon-cli) exits.
    #[cfg(windows)]
    assign_to_job(&child);

    Ok(child)
}

/// On Windows, assign a child process to a Job Object.
/// When the Job Object is closed (parent exits), all children are killed.
#[cfg(windows)]
fn assign_to_job(child: &Child) {
    use std::mem;
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    #[allow(non_camel_case_types)]
    type HANDLE = *mut std::ffi::c_void;
    #[allow(non_camel_case_types)]
    type BOOL = i32;
    const NULL: HANDLE = std::ptr::null_mut();

    #[repr(C)]
    #[allow(non_camel_case_types, non_snake_case)]
    struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION {
        BasicLimitInformation: JOBOBJECT_BASIC_LIMIT_INFORMATION,
        IoInfo: [u64; 3],
        ProcessMemoryLimit: usize,
        JobMemoryLimit: usize,
        PeakProcessMemoryUsed: usize,
        PeakJobMemoryUsed: usize,
    }

    #[repr(C)]
    #[allow(non_camel_case_types, non_snake_case)]
    struct JOBOBJECT_BASIC_LIMIT_INFORMATION {
        PerProcessUserTimeLimit: i64,
        PerJobUserTimeLimit: i64,
        LimitFlags: u32,
        MinimumWorkingSetSize: usize,
        MaximumWorkingSetSize: usize,
        ActiveProcessLimit: u32,
        Affinity: usize,
        PriorityClass: u32,
        SchedulingClass: u32,
    }

    unsafe extern "system" {
        fn CreateJobObjectW(attrs: HANDLE, name: *const u16) -> HANDLE;
        fn SetInformationJobObject(
            job: HANDLE,
            class: u32,
            info: *const u8,
            len: u32,
        ) -> BOOL;
        fn AssignProcessToJobObject(job: HANDLE, process: HANDLE) -> BOOL;
    }

    // Use a single shared Job Object for all children
    static JOB: OnceLock<usize> = OnceLock::new();

    let job_handle = *JOB.get_or_init(|| unsafe {
        let job = CreateJobObjectW(NULL, std::ptr::null());
        if job.is_null() {
            return 0;
        }

        // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x2000
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = mem::zeroed();
        info.BasicLimitInformation.LimitFlags = 0x2000;

        // JobObjectExtendedLimitInformation = 9
        SetInformationJobObject(
            job,
            9,
            &info as *const _ as *const u8,
            mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );

        job as usize
    });

    if job_handle == 0 {
        return;
    }

    unsafe {
        let process_handle = child.as_raw_handle() as HANDLE;
        AssignProcessToJobObject(job_handle as HANDLE, process_handle);
    }
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

    // Kill remaining children
    kill_all(children);

    println!("  {} Stopped.\n", "✓".green());
}

/// Kill all children. On Windows, uses taskkill /T to kill process trees.
fn kill_all(children: &mut Vec<Child>) {
    for child in children.iter_mut() {
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        #[cfg(not(windows))]
        {
            let _ = child.kill();
        }

        let _ = child.wait();
    }
}
