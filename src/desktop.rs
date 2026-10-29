use bevy::{prelude::*, window::RawHandleWrapper};

#[cfg(windows)]
pub fn cursor(raw: &RawHandleWrapper, scale: f32) -> Option<Vec2> {
    use raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::{
        Foundation::POINT, Graphics::Gdi::ScreenToClient, UI::WindowsAndMessaging::GetCursorPos,
    };
    let RawWindowHandle::Win32(handle) = raw.get_window_handle() else {
        return None;
    };
    let mut point = POINT { x: 0, y: 0 };
    // The handle belongs to our own live Bevy window; these calls only read cursor coordinates.
    unsafe {
        if GetCursorPos(&mut point) == 0 || ScreenToClient(handle.hwnd.get() as _, &mut point) == 0
        {
            return None;
        }
    }
    Some(Vec2::new(point.x as f32, point.y as f32) / scale)
}

#[cfg(windows)]
pub fn key_down(code: i32) -> bool {
    unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(code) < 0 }
}

#[cfg(windows)]
pub fn non_activating(raw: &RawHandleWrapper, overlay: bool) {
    use raw_window_handle::RawWindowHandle;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    let RawWindowHandle::Win32(handle) = raw.get_window_handle() else {
        return;
    };
    // Only change our window; keep the existing flags Winit and the compositor require.
    unsafe {
        let hwnd = handle.hwnd.get() as _;
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let flags = (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize;
        let desired = if overlay {
            style | flags
        } else {
            style & !flags
        };
        if style != desired {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
        }
    }
}

#[cfg(not(windows))]
pub fn key_down(_: i32) -> bool {
    false
}
#[cfg(not(windows))]
pub fn cursor(_: &RawHandleWrapper, _: f32) -> Option<Vec2> {
    None
}
#[cfg(not(windows))]
pub fn non_activating(_: &RawHandleWrapper, _: bool) {}
