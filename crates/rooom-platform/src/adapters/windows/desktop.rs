use super::super::claude::{process_info, same_image};
use crate::pipe::Scope;
use std::path::{Path, PathBuf};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM},
    UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GetForegroundWindow, GetWindow, GetWindowThreadProcessId, IsIconic,
        IsWindowVisible, SW_RESTORE, SetForegroundWindow, ShowWindowAsync,
    },
};

#[derive(Debug, PartialEq, Eq)]
pub struct DesktopWindow {
    // The handle travels from the adapter worker to the Windows UI thread.
    handle: usize,
    pub(crate) pid: u32,
    pub(crate) creation: u64,
    executable: PathBuf,
}

impl DesktopWindow {
    pub(crate) fn find(
        scope: &Scope,
        executable: &Path,
        matches: impl Fn(u32) -> bool,
    ) -> Option<Self> {
        let mut search = Search {
            scope,
            executable,
            matches: &matches,
            candidates: Vec::new(),
        };
        let found = unsafe {
            EnumWindows(
                Some(collect_window),
                (&mut search as *mut Search<'_>) as LPARAM,
            )
        };
        if found == 0 || search.candidates.len() != 1 {
            return None;
        }
        search.candidates.pop()
    }

    pub(crate) fn valid(&self, scope: &Scope) -> bool {
        visible_window(self.handle as HWND) == Some(self.pid)
            && scope.owns_process(self.pid).unwrap_or(false)
            && process_info(self.pid).is_ok_and(|process| {
                process.creation_ticks == self.creation
                    && same_image(&process.image, &self.executable)
            })
    }

    pub(crate) fn foreground(
        self,
        unavailable: &'static str,
        denied: &'static str,
    ) -> Result<(), &'static str> {
        let scope = Scope::current().map_err(|_| unavailable)?;
        if !self.valid(&scope) {
            return Err(unavailable);
        }
        let handle = self.handle as HWND;
        // A minimized window still has a visible top-level handle.
        unsafe {
            if IsIconic(handle) != 0 {
                ShowWindowAsync(handle, SW_RESTORE);
            }
            if GetForegroundWindow() == handle || SetForegroundWindow(handle) != 0 {
                Ok(())
            } else {
                Err(denied)
            }
        }
    }
}

fn visible_window(handle: HWND) -> Option<u32> {
    let mut pid = 0;
    unsafe {
        if IsWindowVisible(handle) == 0 || !GetWindow(handle, GW_OWNER).is_null() {
            return None;
        }
        GetWindowThreadProcessId(handle, &mut pid);
    }
    (pid != 0).then_some(pid)
}

struct Search<'a> {
    scope: &'a Scope,
    executable: &'a Path,
    matches: &'a dyn Fn(u32) -> bool,
    candidates: Vec<DesktopWindow>,
}

unsafe extern "system" fn collect_window(handle: HWND, data: LPARAM) -> i32 {
    let search = unsafe { &mut *(data as *mut Search<'_>) };
    if let Some(pid) = visible_window(handle)
        && (search.matches)(pid)
        && search.scope.owns_process(pid).unwrap_or(false)
        && let Ok(process) = process_info(pid)
        && same_image(&process.image, search.executable)
    {
        search.candidates.push(DesktopWindow {
            handle: handle as usize,
            pid,
            creation: process.creation_ticks,
            executable: search.executable.to_owned(),
        });
    }
    1
}
