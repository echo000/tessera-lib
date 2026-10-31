use std::path::Path;

/// System specific workarounds for various issues.
pub fn initialize_workarounds() {
    #[cfg(target_os = "windows")]
    {
        unsafe { std::env::set_var("DISABLE_VULKAN_OBS_CAPTURE", "1") };

        use windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2;
        use windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext;

        // gpui renders per-monitor DPI aware. Binaries embedding their own manifest (e.g. with
        // porter-build, instead of tessera-app's `windows-manifest` feature) may not declare it,
        // so request it here; this fails harmlessly when a manifest already set it.
        // SAFETY: plain call with a predefined context handle, made before any window exists.
        unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    }
}

/// Applies native "always on top" behavior to a GPUI window.
pub fn set_always_on_top(window: &gpui::Window, enabled: bool) -> bool {
    #[cfg(target_os = "windows")]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
        };

        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return false;
        };

        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return false;
        };

        let insert_after = if enabled {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;

        unsafe { SetWindowPos(handle.hwnd.get() as _, insert_after, 0, 0, 0, 0, flags) != 0 }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = (window, enabled);
        false
    }
}

/// Opens a folder in the users file explorer, creating the folder first if it doesn't exist.
pub fn open_folder<F: AsRef<Path>>(folder: F) {
    let folder = folder.as_ref();
    let dirs = std::fs::create_dir_all(folder);

    debug_assert!(dirs.is_ok());

    let folder = folder.to_string_lossy().into_owned();

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;

        use windows_sys::Win32::UI::Shell::*;
        use windows_sys::Win32::UI::WindowsAndMessaging::*;

        let folder: Vec<u16> = std::fs::canonicalize(folder)
            .unwrap_or_default()
            .as_os_str()
            .encode_wide()
            .chain(Some(0x0))
            .collect();

        // SAFETY: The pointer to url lives as long as the call does, and is checked that it's a valid string,
        // in this case we do not care whether or not the call succeeds or fails.
        unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                std::ptr::null(),
                folder.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::process::Command;

        let mut command = Command::new(if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        });

        command.arg(folder);

        let result = command.output();

        debug_assert!(result.is_ok());
    }
}
