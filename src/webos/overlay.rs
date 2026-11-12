//! Makes the SDL window an overlay instead of a full-screen card.
//!
//! SDL-webOS never sets `_WEBOS_WINDOW_TYPE`, so the compositor shows every
//! native SDL window as a card, which replaces the app in front. We set
//! `_WEBOS_WINDOW_TYPE_OVERLAY` on SDL's webOS shell surface right after the
//! window is created and before its first buffer, as web apps get it from
//! `"defaultWindowType": "overlay"`.
//!
//! The shell surface is private to SDL, so it is found through SDL's window
//! data, whose layout is fixed for the SDL-webOS 2.30.12-webos.5 build we
//! ship (SDL_sysvideo.h `SDL_Window::driverdata`, SDL_waylandwindow.h
//! `SDL_WindowData`). Every step is checked: the data must point back at our
//! window, and libwayland must name the two objects `wl_surface` and
//! `wl_webos_shell_surface`. If anything does not match, nothing is touched
//! and the window stays a card.

use libc::{c_char, c_void};
use std::ffi::CStr;

#[link(name = "wayland-client")]
unsafe extern "C" {
    fn wl_proxy_get_class(proxy: *mut c_void) -> *const c_char;
    fn wl_proxy_marshal(proxy: *mut c_void, opcode: u32, ...);
}

/// `wl_webos_shell_surface.set_property` (wayland-webos-shell-client-protocol.h).
const SET_PROPERTY: u32 = 2;
/// Word offsets on 32-bit ARM: `SDL_Window::driverdata` is at byte 164;
/// in `SDL_WindowData`, `sdlwindow` is word 0, `surface` word 2 and the
/// `shell_surface` union starts at word 7, with `webos.webos` its second word.
const DRIVERDATA_BYTES: usize = 164;
const SURFACE_WORD: usize = 2;
const WEBOS_SHELL_WORD: usize = 8;

unsafe fn class_of(proxy: *mut c_void) -> Option<String> {
    if proxy.is_null() || (proxy as usize) % std::mem::align_of::<usize>() != 0 {
        return None;
    }
    // SAFETY: only reached for pointers SDL's own window data holds.
    let c = unsafe { wl_proxy_get_class(proxy) };
    (!c.is_null()).then(|| unsafe { CStr::from_ptr(c) }.to_string_lossy().into_owned())
}

pub fn make_overlay(window: &sdl2::video::Window) -> Result<(), String> {
    if cfg!(not(target_pointer_width = "32")) {
        return Err("overlay: only the 32-bit webOS layout is known".into());
    }
    let win = window.raw() as *const u8;
    // SAFETY: reads inside SDL's live SDL_Window; validated before any use.
    unsafe {
        let data = *(win.add(DRIVERDATA_BYTES) as *const *const *mut c_void);
        if data.is_null() || (data as usize) % 4 != 0 {
            return Err("overlay: no window data".into());
        }
        if *data != win as *mut c_void {
            return Err("overlay: unexpected SDL window layout".into());
        }
        let surface = *data.add(SURFACE_WORD);
        if class_of(surface).as_deref() != Some("wl_surface") {
            return Err("overlay: surface not found".into());
        }
        let shell = *data.add(WEBOS_SHELL_WORD);
        if class_of(shell).as_deref() != Some("wl_webos_shell_surface") {
            return Err("overlay: webOS shell surface not found".into());
        }
        wl_proxy_marshal(shell, SET_PROPERTY, c"_WEBOS_WINDOW_TYPE".as_ptr(), c"_WEBOS_WINDOW_TYPE_OVERLAY".as_ptr());
    }
    Ok(())
}
