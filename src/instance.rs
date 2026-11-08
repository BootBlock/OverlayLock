//! Lets only one lock run at a time in the session (OL16).

use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::w;

/// Holds the session-wide named mutex while this process is the running lock.
#[derive(Debug)]
pub struct InstanceGuard(HANDLE);

impl InstanceGuard {
    /// Takes the named mutex. Returns `Ok(None)` if another instance already holds it.
    pub fn acquire() -> windows::core::Result<Option<Self>> {
        // SAFETY: the name is a static wide string and the default security is used.
        let handle = unsafe {
            CreateMutexW(
                None,
                true,
                w!(r"Local\OverlayLock-6F1D2C4B-9A57-4E3B-8C0E-2B7D5A1F9E63"),
            )
        }?;
        // SAFETY: read at once, before any other call can change the thread's last error.
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            // SAFETY: `handle` was opened above and is closed once.
            let _ = unsafe { CloseHandle(handle) };
            return Ok(None);
        }
        Ok(Some(Self(handle)))
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by `acquire` and is closed once, here.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
