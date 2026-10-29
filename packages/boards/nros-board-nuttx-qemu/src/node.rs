//! Board network read-back for the NuttX QEMU boards (arm virt + rv-virt).
//!
//! NuttX owns this board's network (RFC-0109 D2, `[board.net] stack = "os"`):
//! the kernel brings `eth0` (virtio-net) up during boot, before `main`, from
//! its own `defconfig`'s `CONFIG_NETINIT_*`. nano-ros only READS it.
//!
//! Issue 2048 — this module used to WRITE it: `SIOCSIFADDR` /
//! `SIOCSIFNETMASK` / `SIOCSIFDSTADDR` on `eth0` from `Config::ip()`, plus a
//! `/dev/urandom` reseed from the same four bytes. Both were dead weight by
//! the time they were removed (the issue records the measurement):
//!
//! * the push wrote the value the kernel had already applied — phase-514 W4
//!   generates `BOARD_NET_*` from that same `defconfig`, and no image, fixture
//!   row or entry bake (~~`NROS_IP`~~) states another address;
//! * the reseed fed the IP, which is the SAME `10.0.2.15` in every guest, so
//!   it gave every guest the same seed; and the zenoh session ZID on NuttX is
//!   not drawn from `/dev/urandom` at all (`zpico_fill_session_zid`'s generic
//!   arm: session address, a counter and `CLOCK_REALTIME`).

/// Print the address the kernel gave `eth0`, as read back with `SIOCGIFADDR`.
///
/// This is the read half of RFC-0109 D2 and the boot-log evidence a test
/// asserts on ([`ETH0_REPORT_PREFIX`]): an image whose `defconfig` lost its
/// `CONFIG_NETINIT_*` prints `0.0.0.0` (or the ioctl's error) here instead of
/// only dying later in `Executor::open` as an opaque `ConnectionFailed`.
#[cfg(target_os = "nuttx")]
pub fn report_kernel_eth0() {
    match read_eth0_ipv4() {
        Ok(a) => println!(
            "{ETH0_REPORT_PREFIX}{}.{}.{}.{} (kernel-owned, RFC-0109 D2)",
            a[0], a[1], a[2], a[3]
        ),
        Err(rc) => println!("{ETH0_REPORT_PREFIX}unreadable (SIOCGIFADDR rc={rc})"),
    }
}

/// The boot-log line [`report_kernel_eth0`] prints, up to the address.
/// `nros_tests::output::NUTTX_ETH0_REPORT` mirrors it.
pub const ETH0_REPORT_PREFIX: &str = "nros: eth0 ";

/// `eth0`'s IPv4 address in network order, or the failing call's return code.
#[cfg(target_os = "nuttx")]
fn read_eth0_ipv4() -> Result<[u8; 4], core::ffi::c_int> {
    // phase-359 W7 — `RawFd` is an alias for `c_int`; the alias was the only
    // thing std supplied here.
    type RawFd = core::ffi::c_int;

    // NuttX defines this as `_SIOC(N) = 0x0700 | N`, *not* the Linux 0x89xx
    // range (`nuttx/include/nuttx/net/ioctl.h`: `SIOCGIFADDR _SIOC(0x0001)`).
    const SIOCGIFADDR: core::ffi::c_ulong = 0x0700 | 0x0001;

    #[repr(C)]
    struct sockaddr_in {
        sin_family: u16,
        sin_port: u16,
        sin_addr: u32,
        sin_zero: [u8; 8],
    }
    #[repr(C)]
    struct ifreq {
        ifr_name: [u8; 16],
        ifr_addr: sockaddr_in,
    }
    unsafe extern "C" {
        fn socket(domain: core::ffi::c_int, ty: core::ffi::c_int, proto: core::ffi::c_int)
        -> RawFd;
        fn ioctl(fd: RawFd, req: core::ffi::c_ulong, ...) -> core::ffi::c_int;
        fn close(fd: RawFd) -> core::ffi::c_int;
    }
    const AF_INET: core::ffi::c_int = 2;
    const SOCK_DGRAM: core::ffi::c_int = 2;

    // SAFETY: plain libc calls; `req` is a properly laid-out `ifreq` we own,
    // and the fd is closed on every path after a successful `socket`.
    let fd = unsafe { socket(AF_INET, SOCK_DGRAM, 0) };
    if fd < 0 {
        return Err(fd);
    }
    let mut name = [0u8; 16];
    name[..4].copy_from_slice(b"eth0");
    let mut req = ifreq {
        ifr_name: name,
        ifr_addr: sockaddr_in {
            sin_family: AF_INET as u16,
            sin_port: 0,
            sin_addr: 0,
            sin_zero: [0; 8],
        },
    };
    let rc = unsafe { ioctl(fd, SIOCGIFADDR, &mut req) };
    unsafe {
        close(fd);
    }
    if rc < 0 {
        return Err(rc);
    }
    // Network byte order in memory: the first octet is the lowest byte.
    Ok(req.ifr_addr.sin_addr.to_ne_bytes())
}

// Phase 313 W-nuttx (#0243) — the legacy free `run(Config, closure)` (a session
// opener over the retired ~~`nros_board_common::board_init`~~ traits) is GONE. The
// live entry is `<NuttxQemu as nros_platform::board::BoardEntry>::run`
// (entry_212n.rs), used by `nros::main!`; NuttX dispatches the logging/init-only
// fixtures' plain `fn main` directly via `nsh_main`, so they need no board entry.
