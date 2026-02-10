//! Self-pipe wakeup mechanism for event-driven main loop.
//!
//! Provides a `WakeupSender` (Clone, write end) and `WakeupReceiver` (read end)
//! backed by an OS pipe. The sender writes a single byte to wake up a thread
//! blocked in `libc::poll()`.

use std::io;
use std::os::unix::io::RawFd;
use std::sync::Arc;

/// Write end of the wakeup pipe. Cloneable — hand out copies to any thread
/// that needs to wake the main loop. The underlying fd is reference-counted
/// and only closed when the last clone is dropped.
#[derive(Clone)]
pub struct WakeupSender {
    fd: Arc<OwnedFd>,
}

/// Read end of the wakeup pipe. Exposes `raw_fd()` for use with `libc::poll()`
/// and `drain()` to consume all pending wakeup bytes.
pub struct WakeupReceiver {
    fd: OwnedFd,
}

/// RAII wrapper for a raw fd that closes on drop.
struct OwnedFd(RawFd);

impl Drop for OwnedFd {
    fn drop(&mut self) {
        // SAFETY: fd is a valid pipe end created by pipe().
        unsafe {
            libc::close(self.0);
        }
    }
}

// SAFETY: The raw fd is just an integer handle; sending across threads is safe.
unsafe impl Send for OwnedFd {}
unsafe impl Sync for OwnedFd {}

/// Create a wakeup pipe pair.
///
/// The read end is set to non-blocking so `drain()` never blocks.
pub fn wakeup_pipe() -> io::Result<(WakeupSender, WakeupReceiver)> {
    let mut fds = [0 as RawFd; 2];
    // SAFETY: fds is a valid 2-element array.
    let ret = unsafe { libc::pipe(fds.as_mut_ptr()) };
    if ret != 0 {
        return Err(io::Error::last_os_error());
    }
    let read_fd = fds[0];
    let write_fd = fds[1];

    // Set read end to non-blocking.
    // SAFETY: read_fd is a valid fd just created by pipe().
    unsafe {
        let flags = libc::fcntl(read_fd, libc::F_GETFL);
        if flags == -1 {
            let err = io::Error::last_os_error();
            libc::close(read_fd);
            libc::close(write_fd);
            return Err(err);
        }
        if libc::fcntl(read_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) == -1 {
            let err = io::Error::last_os_error();
            libc::close(read_fd);
            libc::close(write_fd);
            return Err(err);
        }
    }

    Ok((
        WakeupSender {
            fd: Arc::new(OwnedFd(write_fd)),
        },
        WakeupReceiver {
            fd: OwnedFd(read_fd),
        },
    ))
}

impl WakeupSender {
    /// Write a single byte to wake the polling thread.
    /// Errors (EAGAIN, BrokenPipe) are silently ignored — the wakeup is
    /// best-effort and the pipe may already be full or the receiver dropped.
    pub fn wake(&self) {
        // SAFETY: fd is a valid pipe write end; buf is a valid 1-byte slice.
        unsafe {
            libc::write(self.fd.0, [1u8].as_ptr().cast(), 1);
        }
    }
}

impl WakeupReceiver {
    /// Return the raw fd for use with `libc::poll()`.
    pub fn raw_fd(&self) -> RawFd {
        self.fd.0
    }

    /// Drain all pending bytes from the pipe (non-blocking).
    pub fn drain(&self) {
        let mut buf = [0u8; 64];
        loop {
            // SAFETY: fd is a valid non-blocking pipe read end; buf is valid.
            let n = unsafe { libc::read(self.fd.0, buf.as_mut_ptr().cast(), buf.len()) };
            if n <= 0 {
                break;
            }
        }
    }
}
