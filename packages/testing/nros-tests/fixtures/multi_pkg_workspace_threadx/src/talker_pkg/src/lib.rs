// Phase 212.H.4 fixture — talker component (Rust staticlib).
//
// Phase 212.M.5.a.1 — exposes the canonical per-pkg mangled
// `__nros_component_talker_pkg_register` entry that the codegen-emitted
// `system_main.c` calls. Real Phase 212 components will use
// `nros::component!()` to emit this symbol; this fixture hand-writes
// the export to keep the build-path + corrosion-bridge audit scoped
// (no nros workspace pull-in).

#[unsafe(no_mangle)]
pub extern "C" fn __nros_component_talker_pkg_register(_ctx: *mut core::ffi::c_void) {
    // Print without using the standard library or libc bindings to
    // keep the staticlib usable from any cmake host (Linux x86_64
    // here, ThreadX-Linux native simulation).
    let msg = b"[talker] component entry reached\n\0";
    unsafe extern "C" {
        fn fputs(s: *const u8, stream: *mut core::ffi::c_void) -> i32;
        static stdout: *mut core::ffi::c_void;
    }
    unsafe {
        let _ = fputs(msg.as_ptr(), stdout);
    }
}
