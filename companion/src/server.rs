//! `start` and `stop`: run ComfyUI from a folder the companion installed, in the foreground, and
//! stop it again. The pid lives in `comfyui.pid` beside it.

use std::fs;
use std::net::{IpAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::install;
use crate::installed;

/// Runs ComfyUI from `dir` and waits for it; the companion exits with ComfyUI's code.
pub fn start(dir: &Path, port: u16, lan: bool) -> Result<ExitCode, String> {
    installed::read(dir).map_err(|e| format!("{e}\nrun install first, or pass --dir to the folder it made"))?;

    let main = canonical(install::comfyui_dir(dir).join("main.py"));
    if !main.is_file() {
        return Err(format!("{}: missing; run install again", main.display()));
    }
    let python = install::venv_python(dir);
    if !python.is_file() {
        return Err(format!("{}: missing; run install again", python.display()));
    }

    let pid_file = pid_path(dir);
    if let Some(pid) = read_pid(&pid_file)? {
        if is_running(pid) {
            return Err(format!("already running (pid {pid})"));
        }
        let _ = fs::remove_file(&pid_file);
    }

    if lan {
        println!("warning: ComfyUI has no login: anyone on the network can use it");
    }
    let mut command = Command::new(&python);
    command
        .arg(&main)
        .arg("--port")
        .arg(port.to_string())
        .arg("--preview-method")
        .arg("auto")
        .current_dir(install::comfyui_dir(dir));
    if lan {
        command.arg("--listen").arg("0.0.0.0");
    }

    let mut child = command.spawn().map_err(|e| format!("{}: {e}", python.display()))?;
    // Ctrl+C goes to the whole process group, so ComfyUI gets it too; leaving it alone here would
    // end the companion first, before it could wait for ComfyUI or remove the pid file.
    ignore_interrupt();
    if let Err(e) = fs::write(&pid_file, format!("{}\n", child.id())) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("{}: {e}", pid_file.display()));
    }

    if lan {
        match lan_address() {
            Some(address) => println!("ComfyUI is at http://{address}:{port}"),
            None => {
                println!("ComfyUI is listening on all addresses, port {port}; couldn't find this machine's LAN address")
            }
        }
    } else {
        for line in local_address_lines(port) {
            println!("{line}");
        }
    }

    let status = child.wait();
    let _ = fs::remove_file(&pid_file);
    Ok(exit_code(status.map_err(|e| format!("waiting for ComfyUI: {e}"))?))
}

/// Stops the ComfyUI `start` ran from `dir`, if it is still running.
pub fn stop(dir: &Path) -> Result<(), String> {
    let pid_file = pid_path(dir);
    let Some(pid) = read_pid(&pid_file)? else {
        let _ = fs::remove_file(&pid_file);
        println!("not running");
        return Ok(());
    };
    if !is_running(pid) {
        let _ = fs::remove_file(&pid_file);
        println!("not running");
        return Ok(());
    }
    if !is_ours(dir, pid) {
        let _ = fs::remove_file(&pid_file);
        println!("not running: pid {pid} is not this folder's ComfyUI");
        return Ok(());
    }

    println!("Stopping ComfyUI (pid {pid})...");
    send_interrupt(pid)?;
    if !wait_gone(pid, 10) {
        println!("Still running; terminating.");
        send_terminate(pid)?;
        if !wait_gone(pid, 5) {
            let _ = fs::remove_file(&pid_file);
            return Err(format!("could not stop pid {pid}"));
        }
    }
    let _ = fs::remove_file(&pid_file);
    println!("Stopped.");
    Ok(())
}

fn pid_path(dir: &Path) -> PathBuf {
    dir.join("comfyui.pid")
}

/// The pid in `path`, or nothing when the file is missing or doesn't hold one.
fn read_pid(path: &Path) -> Result<Option<u32>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(text.trim().parse::<u32>().ok()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn canonical(path: PathBuf) -> PathBuf {
    path.canonicalize().unwrap_or(path)
}

/// Whether a process with `pid` is running.
#[cfg(unix)]
fn is_running(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(windows)]
fn is_running(pid: u32) -> bool {
    let filter = format!("PID eq {pid}");
    Command::new("tasklist")
        .args(["/FI", &filter, "/NH"])
        .output()
        .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains(&pid.to_string()))
}

/// Whether the process is the ComfyUI from this folder. Linux only: elsewhere the pid file is
/// trusted, because there is no command line as easy to read.
fn is_ours(dir: &Path, pid: u32) -> bool {
    if !cfg!(target_os = "linux") {
        return true;
    }
    let main = canonical(install::comfyui_dir(dir).join("main.py"));
    let expected = main.to_string_lossy().to_string();
    let Ok(bytes) = fs::read(format!("/proc/{pid}/cmdline")) else { return false };
    String::from_utf8_lossy(&bytes).replace('\0', " ").contains(&expected)
}

/// Waits up to `seconds` for the process to end.
fn wait_gone(pid: u32, seconds: u64) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(seconds) {
        if !is_running(pid) {
            return true;
        }
        thread::sleep(Duration::from_millis(250));
    }
    !is_running(pid)
}

#[cfg(unix)]
fn send_interrupt(pid: u32) -> Result<(), String> {
    send_signal("-INT", pid)
}

#[cfg(unix)]
fn send_terminate(pid: u32) -> Result<(), String> {
    send_signal("-TERM", pid)
}

#[cfg(unix)]
fn send_signal(signal: &str, pid: u32) -> Result<(), String> {
    let status = Command::new("kill")
        .args([signal, &pid.to_string()])
        .status()
        .map_err(|e| format!("kill {signal} {pid}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("kill {signal} {pid}: {status}")) }
}

#[cfg(windows)]
fn send_interrupt(pid: u32) -> Result<(), String> {
    taskkill(pid, false)
}

#[cfg(windows)]
fn send_terminate(pid: u32) -> Result<(), String> {
    taskkill(pid, true)
}

#[cfg(windows)]
fn taskkill(pid: u32, force: bool) -> Result<(), String> {
    let mut command = Command::new("taskkill");
    command.arg("/PID").arg(pid.to_string());
    if force {
        command.arg("/F");
    }
    let status = command.status().map_err(|e| format!("taskkill /PID {pid}: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("taskkill /PID {pid}: {status}")) }
}

/// Ignore Ctrl+C in the companion, so it can wait for ComfyUI and leave with its exit code. The
/// terminal sends the signal to the whole process group, so the child stops too; this runs after it
/// starts, so the child keeps the default behaviour.
#[cfg(unix)]
#[allow(unsafe_code)] // std has no signal handling: this is the only platform call needed
fn ignore_interrupt() {
    // SIGINT is 2 and SIG_IGN is 1 on every Unix the companion runs on.
    unsafe extern "C" {
        fn signal(signum: i32, handler: usize) -> usize;
    }
    // SAFETY: `signal` only records a disposition for this process, and the two constants are the
    // C library's own.
    unsafe {
        signal(2, 1);
    }
}

/// Ignore Ctrl+C in the companion, so it can wait for ComfyUI and leave with its exit code. Windows
/// sends the event to every process attached to the console, so the child stops too; this runs
/// after it starts, because the ignore flag would otherwise be inherited by it.
#[cfg(windows)]
#[allow(unsafe_code)] // std has no console handling: this is the one platform call
fn ignore_interrupt() {
    use std::ffi::c_void;
    unsafe extern "system" {
        fn SetConsoleCtrlHandler(handler: *const c_void, add: i32) -> i32;
    }
    // SAFETY: a null handler with `add` set to 1 is the documented way to ignore Ctrl+C for this
    // process; the return value only says whether it worked.
    unsafe {
        SetConsoleCtrlHandler(std::ptr::null(), 1);
    }
}

/// The machine's LAN address: found by asking the routing table where a public address would go,
/// without sending anything.
fn lan_address() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|address| address.ip())
}

/// What to type on the tablet when the server only listens on this machine.
fn local_address_lines(port: u16) -> [String; 2] {
    [format!("adb reverse tcp:{port} tcp:{port}"), format!("127.0.0.1:{port}")]
}

fn exit_code(status: ExitStatus) -> ExitCode {
    if let Some(code) = status.code() {
        return ExitCode::from(u8::try_from(code).unwrap_or(1));
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return ExitCode::from(u8::try_from(128 + signal).unwrap_or(1));
        }
    }
    ExitCode::from(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_dir() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "procraft-companion-server-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_pid_file_is_read_or_ignored() {
        let dir = temp_dir();
        let path = dir.join("comfyui.pid");
        fs::write(&path, b"1234\n").unwrap();
        assert_eq!(read_pid(&path).unwrap(), Some(1234));
        fs::write(&path, b"not a pid").unwrap();
        assert_eq!(read_pid(&path).unwrap(), None);
        let _ = fs::remove_file(&path);
        assert_eq!(read_pid(&path).unwrap(), None);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_tablet_address_is_the_local_one() {
        assert_eq!(
            local_address_lines(8189),
            ["adb reverse tcp:8189 tcp:8189".to_owned(), "127.0.0.1:8189".to_owned()]
        );
    }
}
