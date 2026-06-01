// Phase 212.H.4 fixture — listener component (Rust staticlib).

#[unsafe(no_mangle)]
pub extern "C" fn nros_component_listener_entry() {
    let msg = b"[listener] component entry reached\n\0";
    unsafe extern "C" {
        fn fputs(s: *const u8, stream: *mut core::ffi::c_void) -> i32;
        static stdout: *mut core::ffi::c_void;
    }
    unsafe {
        let _ = fputs(msg.as_ptr(), stdout);
    }
}
