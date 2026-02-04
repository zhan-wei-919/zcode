#![cfg(all(unix, feature = "tui"))]

use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::io::{FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tempfile::tempdir;

struct PtyChild {
    master: Option<File>,
    child: Child,
    buffer: Vec<u8>,
}

impl PtyChild {
    fn spawn(bin: &Path, args: &[PathBuf], env: &[(&str, &Path)]) -> std::io::Result<Self> {
        let (master_fd, slave_fd) = open_pty()?;
        set_winsize(slave_fd, 80, 24)?;
        set_nonblocking(master_fd)?;

        let mut cmd = Command::new(bin);
        for arg in args {
            cmd.arg(arg);
        }
        for (key, value) in env {
            cmd.env(key, value);
        }

        // SAFETY: `pre_exec` runs in the child process after `fork` and before `exec`.
        // We only call libc functions and immediately return errors; no allocations or locks.
        unsafe {
            let slave = slave_fd;
            cmd.pre_exec(move || {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(slave, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        // SAFETY: `from_raw_fd` takes ownership; we pass duplicated fds so the original
        // `slave_fd` can be closed safely after spawning the child.
        let stdin = unsafe { Stdio::from_raw_fd(dup_fd(slave_fd)?) };
        let stdout = unsafe { Stdio::from_raw_fd(dup_fd(slave_fd)?) };
        let stderr = unsafe { Stdio::from_raw_fd(dup_fd(slave_fd)?) };
        cmd.stdin(stdin).stdout(stdout).stderr(stderr);

        let child = cmd.spawn()?;
        unsafe {
            libc::close(slave_fd);
        }

        let master = unsafe { File::from_raw_fd(master_fd) };
        Ok(Self {
            master: Some(master),
            child,
            buffer: Vec::new(),
        })
    }

    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let master = self
            .master
            .as_mut()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pty closed"))?;
        master.write_all(bytes)?;
        master.flush()
    }

    fn read_into_buffer(&mut self) -> std::io::Result<usize> {
        let mut chunk = [0u8; 8192];
        let master = self
            .master
            .as_mut()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pty closed"))?;
        match master.read(&mut chunk) {
            Ok(0) => Ok(0),
            Ok(n) => {
                self.buffer.extend_from_slice(&chunk[..n]);
                Ok(n)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(0),
            Err(e) => Err(e),
        }
    }

    fn clear_buffer(&mut self) {
        self.buffer.clear();
    }

    fn wait_for_output(&mut self, needle: &[u8], timeout: Duration) -> std::io::Result<()> {
        let start = Instant::now();
        loop {
            let _ = self.read_into_buffer()?;
            if self.buffer.windows(needle.len()).any(|w| w == needle) {
                return Ok(());
            }
            if start.elapsed() > timeout {
                let tail_start = self.buffer.len().saturating_sub(4000);
                let tail = String::from_utf8_lossy(&self.buffer[tail_start..]).to_string();
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!(
                        "timeout waiting for output; collected {} bytes\n\noutput tail:\n{}",
                        self.buffer.len(),
                        tail
                    ),
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_exit(&mut self, timeout: Duration) -> std::io::Result<()> {
        let start = Instant::now();
        loop {
            if let Some(_status) = self.child.try_wait()? {
                return Ok(());
            }
            if start.elapsed() > timeout {
                // Fall back to a PTY hangup (close master) instead of signals.
                drop(self.master.take());

                let hangup_start = Instant::now();
                while hangup_start.elapsed() < Duration::from_secs(2) {
                    if let Some(_status) = self.child.try_wait()? {
                        return Ok(());
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                return Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "timeout waiting for child to exit (pty hangup failed)",
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

fn open_pty() -> std::io::Result<(RawFd, RawFd)> {
    unsafe {
        let master = libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY);
        if master < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::grantpt(master) != 0 {
            let err = std::io::Error::last_os_error();
            libc::close(master);
            return Err(err);
        }
        if libc::unlockpt(master) != 0 {
            let err = std::io::Error::last_os_error();
            libc::close(master);
            return Err(err);
        }

        let name_ptr = libc::ptsname(master);
        if name_ptr.is_null() {
            let err = std::io::Error::last_os_error();
            libc::close(master);
            return Err(err);
        }
        let cstr = CStr::from_ptr(name_ptr);
        let slave = libc::open(cstr.as_ptr(), libc::O_RDWR | libc::O_NOCTTY);
        if slave < 0 {
            let err = std::io::Error::last_os_error();
            libc::close(master);
            return Err(err);
        }

        Ok((master, slave))
    }
}

fn dup_fd(fd: RawFd) -> std::io::Result<RawFd> {
    unsafe {
        let duped = libc::dup(fd);
        if duped < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(duped)
    }
}

fn set_nonblocking(fd: RawFd) -> std::io::Result<()> {
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

fn set_winsize(fd: RawFd, cols: u16, rows: u16) -> std::io::Result<()> {
    unsafe {
        let ws = libc::winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let rc = libc::ioctl(fd, libc::TIOCSWINSZ as _, &ws);
        if rc < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

fn wait_for_file_prefix(path: &Path, prefix: &str, timeout: Duration) {
    let start = Instant::now();
    loop {
        if let Ok(content) = std::fs::read_to_string(path) {
            if content.starts_with(prefix) {
                return;
            }
        }
        if start.elapsed() > timeout {
            panic!("timeout waiting for file to start with {prefix:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn tui_can_open_edit_save_and_quit_in_real_tty() {
    // This test requires a working PTY (/dev/ptmx) and enough permissions to create a
    // controlling TTY. In some CI/sandbox environments it will fail with EPERM/EACCES.
    // Run it explicitly when you have a real TTY environment available.
    if std::env::var("ZCODE_RUN_TUI_E2E").ok().as_deref() != Some("1") {
        eprintln!("skipping tui e2e; set ZCODE_RUN_TUI_E2E=1 to enable");
        return;
    }

    let workspace = tempdir().unwrap();
    let file_path = workspace.path().join("a.txt");
    std::fs::write(&file_path, "hello\n").unwrap();

    let home_dir = tempdir().unwrap();

    let bin = PathBuf::from(env!("CARGO_BIN_EXE_zcode"));
    let mut pty = PtyChild::spawn(
        &bin,
        &[workspace.path().to_path_buf()],
        &[
            ("HOME", home_dir.path()),
            ("TERM", Path::new("xterm-256color")),
            ("ZCODE_DISABLE_SETTINGS", Path::new("1")),
            ("ZCODE_DISABLE_LSP", Path::new("1")),
        ],
    )
    .unwrap();

    pty.wait_for_output(b"\x1b[?1049h", Duration::from_secs(3))
        .unwrap();
    pty.clear_buffer();

    // Open command palette (F1), run "View: Focus Explorer".
    pty.write_all(b"\x1bOP").unwrap();
    pty.wait_for_output(b"Command Palette", Duration::from_secs(2))
        .unwrap();
    pty.write_all(b"explorer").unwrap();
    pty.wait_for_output(b"explorer", Duration::from_secs(2))
        .unwrap();
    pty.write_all(b"\r").unwrap();
    std::thread::sleep(Duration::from_millis(30));

    // Select the first entry and open it.
    pty.clear_buffer();
    pty.write_all(b"\x1b[B").unwrap(); // Down
    pty.write_all(b"\r").unwrap(); // Enter
    pty.wait_for_output(b"hello", Duration::from_secs(3))
        .unwrap();

    // Edit + save.
    pty.write_all(b"X").unwrap();
    pty.write_all(&[0x13]).unwrap(); // Ctrl+S
    wait_for_file_prefix(&file_path, "X", Duration::from_secs(3));

    // Quit via command palette (avoids Ctrl+Q/XON quirks in PTYs).
    pty.clear_buffer();
    pty.write_all(b"\x1bOP").unwrap(); // F1
    pty.wait_for_output(b"Command Palette", Duration::from_secs(2))
        .unwrap();
    pty.write_all(b"quit").unwrap();
    pty.wait_for_output(b"quit", Duration::from_secs(2))
        .unwrap();
    pty.write_all(b"\r").unwrap();
    pty.wait_exit(Duration::from_secs(5)).unwrap();
}
