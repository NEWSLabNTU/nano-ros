//! Build script for nuttx-sys.
//!
//! Runs bindgen to generate Rust FFI bindings from NuttX POSIX headers.
//! Requires: NUTTX_DIR — NuttX RTOS source root (contains include/).

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=NUTTX_DIR");

    let nuttx_dir = match env::var("NUTTX_DIR") {
        Ok(d) => d,
        Err(_) => {
            emit_placeholder_bindings();
            return;
        }
    };

    let nuttx = PathBuf::from(&nuttx_dir);
    let nuttx_include = nuttx.join("include");
    if !nuttx_include.exists() {
        emit_placeholder_bindings();
        return;
    }

    let target = env::var("TARGET").unwrap_or_default();

    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .use_core()
        .ctypes_prefix("core::ffi")
        .clang_arg(format!("-I{}", nuttx_include.display()))
        // NuttX cross-compilation target
        .clang_arg("--target=arm-none-eabi")
        .clang_arg("-mthumb")
        .clang_arg("-march=armv7-m")
        // NuttX define
        .clang_arg("-D__NuttX__")
        // Types
        .allowlist_type("timeval")
        .allowlist_type("addrinfo")
        .allowlist_type("sockaddr")
        .allowlist_type("sockaddr_in")
        .allowlist_type("sockaddr_in6")
        .allowlist_type("sockaddr_storage")
        .allowlist_type("linger")
        // Phase 97.4.nuttx — IGMP join via setsockopt
        .allowlist_type("ip_mreq")
        .allowlist_type("in_addr")
        // Socket functions
        .allowlist_function("socket")
        .allowlist_function("connect")
        .allowlist_function("bind")
        .allowlist_function("listen")
        .allowlist_function("accept")
        .allowlist_function("recv")
        .allowlist_function("recvfrom")
        .allowlist_function("send")
        .allowlist_function("sendto")
        .allowlist_function("setsockopt")
        .allowlist_function("getsockopt")
        .allowlist_function("close")
        .allowlist_function("shutdown")
        .allowlist_function("fcntl")
        .allowlist_function("getaddrinfo")
        .allowlist_function("freeaddrinfo")
        .allowlist_function("select")
        // Constants
        .allowlist_var("AF_INET")
        .allowlist_var("AF_INET6")
        .allowlist_var("AF_UNSPEC")
        .allowlist_var("PF_UNSPEC")
        .allowlist_var("SOCK_STREAM")
        .allowlist_var("SOCK_DGRAM")
        .allowlist_var("IPPROTO_TCP")
        .allowlist_var("IPPROTO_UDP")
        .allowlist_var("SOL_SOCKET")
        .allowlist_var("SO_RCVTIMEO")
        .allowlist_var("SO_SNDTIMEO")
        .allowlist_var("SO_KEEPALIVE")
        .allowlist_var("SO_REUSEADDR")
        .allowlist_var("SO_LINGER")
        .allowlist_var("TCP_NODELAY")
        .allowlist_var("O_NONBLOCK")
        .allowlist_var("F_GETFL")
        .allowlist_var("F_SETFL")
        .allowlist_var("SHUT_RDWR")
        .allowlist_var("MSG_NOSIGNAL")
        // Phase 97.4.nuttx — IGMP + RTPS literal-only resolution
        .allowlist_var("IP_ADD_MEMBERSHIP")
        .allowlist_var("IP_DROP_MEMBERSHIP")
        .allowlist_var("INADDR_ANY")
        .allowlist_var("IPPROTO_IP")
        .allowlist_var("AI_NUMERICHOST")
        // No layout tests
        .layout_tests(false)
        .derive_debug(false)
        .derive_default(true)
        .derive_copy(true);

    // NuttX ARM cross-compilation needs GCC sysroot for stdint.h etc.
    if target.contains("thumbv7") || target.contains("arm") {
        if let Some(gcc_include) = find_gcc_include() {
            let gcc_base = PathBuf::from(&gcc_include)
                .parent()
                .unwrap_or(std::path::Path::new(""))
                .to_path_buf();
            let gcc_include_fixed = gcc_base.join("include-fixed");
            builder = builder
                .clang_arg("-nostdinc")
                .clang_arg(format!("-isystem{gcc_include}"))
                .clang_arg(format!("-isystem{}", gcc_include_fixed.display()));
        }
    }

    let bindings = builder
        .generate()
        .expect("Failed to generate NuttX bindings");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Failed to write bindings");
}

fn find_gcc_include() -> Option<String> {
    let output = std::process::Command::new("arm-none-eabi-gcc")
        .args(["-print-file-name=include"])
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() || path == "include" {
        None
    } else {
        Some(path)
    }
}

fn emit_placeholder_bindings() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    std::fs::write(
        out_dir.join("bindings.rs"),
        r#"
// Placeholder — NuttX headers not available.
// Set NUTTX_DIR for real bindings.

pub type time_t = i32;
pub type suseconds_t = i32;
pub type socklen_t = u32;
pub type sa_family_t = u8;

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct timeval { pub tv_sec: time_t, pub tv_usec: suseconds_t }

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct addrinfo {
    pub ai_flags: core::ffi::c_int,
    pub ai_family: core::ffi::c_int,
    pub ai_socktype: core::ffi::c_int,
    pub ai_protocol: core::ffi::c_int,
    pub ai_addrlen: socklen_t,
    pub ai_addr: *mut sockaddr,
    pub ai_canonname: *mut core::ffi::c_char,
    pub ai_next: *mut addrinfo,
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct sockaddr { pub sa_family: sa_family_t, pub sa_data: [core::ffi::c_char; 14] }

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct sockaddr_in {
    pub sin_family: sa_family_t,
    pub sin_port: u16,
    pub sin_addr: in_addr,
    pub sin_zero: [u8; 8],
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct in_addr { pub s_addr: u32 }

#[repr(C)]
#[derive(Default, Copy, Clone)]
pub struct linger { pub l_onoff: core::ffi::c_int, pub l_linger: core::ffi::c_int }

pub const AF_UNSPEC: u32 = 0;
pub const AF_INET: u32 = 2;
pub const PF_UNSPEC: u32 = 0;
pub const SOCK_STREAM: u32 = 1;
pub const SOCK_DGRAM: u32 = 2;
pub const IPPROTO_TCP: u32 = 6;
pub const IPPROTO_UDP: u32 = 17;
pub const SOL_SOCKET: u32 = 0xFFFF;
pub const SO_KEEPALIVE: u32 = 8;
pub const SO_LINGER: u32 = 128;
pub const SO_RCVTIMEO: u32 = 0x1006;
pub const SO_SNDTIMEO: u32 = 0x1005;
pub const SO_REUSEADDR: u32 = 4;
pub const TCP_NODELAY: u32 = 1;
pub const F_GETFL: u32 = 3;
pub const F_SETFL: u32 = 4;
pub const O_NONBLOCK: u32 = 1;
pub const SHUT_RDWR: u32 = 2;
pub const MSG_NOSIGNAL: u32 = 0;

unsafe extern "C" {
    pub fn socket(domain: core::ffi::c_int, ty: core::ffi::c_int, protocol: core::ffi::c_int) -> core::ffi::c_int;
    pub fn connect(fd: core::ffi::c_int, addr: *const sockaddr, addrlen: socklen_t) -> core::ffi::c_int;
    pub fn bind(fd: core::ffi::c_int, addr: *const sockaddr, addrlen: socklen_t) -> core::ffi::c_int;
    pub fn listen(fd: core::ffi::c_int, backlog: core::ffi::c_int) -> core::ffi::c_int;
    pub fn accept(fd: core::ffi::c_int, addr: *mut sockaddr, addrlen: *mut socklen_t) -> core::ffi::c_int;
    pub fn recv(fd: core::ffi::c_int, buf: *mut core::ffi::c_void, len: usize, flags: core::ffi::c_int) -> isize;
    pub fn recvfrom(fd: core::ffi::c_int, buf: *mut core::ffi::c_void, len: usize, flags: core::ffi::c_int, addr: *mut sockaddr, addrlen: *mut socklen_t) -> isize;
    pub fn send(fd: core::ffi::c_int, buf: *const core::ffi::c_void, len: usize, flags: core::ffi::c_int) -> isize;
    pub fn sendto(fd: core::ffi::c_int, buf: *const core::ffi::c_void, len: usize, flags: core::ffi::c_int, addr: *const sockaddr, addrlen: socklen_t) -> isize;
    pub fn setsockopt(fd: core::ffi::c_int, level: core::ffi::c_int, optname: core::ffi::c_int, optval: *const core::ffi::c_void, optlen: socklen_t) -> core::ffi::c_int;
    pub fn close(fd: core::ffi::c_int) -> core::ffi::c_int;
    pub fn shutdown(fd: core::ffi::c_int, how: core::ffi::c_int) -> core::ffi::c_int;
    pub fn fcntl(fd: core::ffi::c_int, cmd: core::ffi::c_int, ...) -> core::ffi::c_int;
    pub fn getaddrinfo(node: *const core::ffi::c_char, service: *const core::ffi::c_char, hints: *const addrinfo, res: *mut *mut addrinfo) -> core::ffi::c_int;
    pub fn freeaddrinfo(ai: *mut addrinfo);
    pub fn select(nfds: core::ffi::c_int, readfds: *mut core::ffi::c_void, writefds: *mut core::ffi::c_void, exceptfds: *mut core::ffi::c_void, timeout: *mut timeval) -> core::ffi::c_int;
}
"#,
    )
    .unwrap();
}
