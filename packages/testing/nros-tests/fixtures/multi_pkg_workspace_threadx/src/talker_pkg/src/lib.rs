// Phase 212.H.4 fixture — talker component (Rust staticlib).
//
// Minimal component: exposes a single `extern "C"` entry that
// system_main.c calls from the ThreadX app thread. Real Phase 212
// components will be `#[nros::component]`-derived; this stub keeps
// the test scoped to the build-path + corrosion-bridge audit.

#[unsafe(no_mangle)]
pub extern "C" fn nros_component_talker_entry() {
    // Print without using the standard library or libc bindings to
    // keep the staticlib usable from any cmake host (Linux x86_64
    // here, ThreadX-Linux native simulation).
    let msg = b"[talker] component entry reached\n\0";
    unsafe {
        extern "C" {
            fn fputs(s: *const u8, stream: *mut core::ffi::c_void) -> i32;
            static stdout: *mut core::ffi::c_void;
        }
        let _ = fputs(msg.as_ptr(), stdout);
    }
}
