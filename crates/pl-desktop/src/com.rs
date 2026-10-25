//! COM for the shell's lookups (the desktop shortcut's shell link, the `discord:` link handler).
//! Each lookup gets an apartment of its own on a fresh thread (a caller's apartment is never
//! borrowed), and only one such apartment exists at a time in this process: two set up and torn
//! down side by side let the shell's own code meet one of them stopping in the middle of a call
//! (`CO_E_SERVER_STOPPING`), which it answers by ending the whole process (seen in
//! `Windows.FileExplorer.Common.dll` while the tests ran on a loaded machine, 25 Oct 2026).

use std::sync::{Mutex, PoisonError};

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

/// Runs `job` on a new thread in a single-threaded apartment of its own, once no other job of
/// this process is in one. `None` when the thread stopped (the job panicked). It blocks the caller
/// until then, also behind another lookup: call it from a blocking thread (the launcher's callers
/// use `spawn_blocking`), never from the UI thread or from inside a job (that would wait for
/// itself).
pub(crate) fn in_apartment<T: Send + 'static>(
    job: impl FnOnce() -> T + Send + 'static,
) -> Option<T> {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    std::thread::spawn(move || {
        // Held until the apartment is gone again (dropped last).
        let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner);
        // SAFETY: initializes COM for this thread only; balanced below when it succeeded.
        let init = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let result = job();
        if init.is_ok() {
            // SAFETY: matches the successful CoInitializeEx above on the same thread.
            unsafe { CoUninitialize() };
        }
        result
    })
    .join()
    .ok()
}
