//! Issue 2950 -- the connect budget bounds a TCP connect to a peer that drops
//! the SYN, in all three of its modes, through the real C shim and zenoh-pico.
//!
//! Before the fix, budget 0 ("one attempt") and forever both connected with a
//! plain blocking `connect()`, so an unreachable address held the open for the
//! host's SYN-retry limit (about 127 s on Linux), whatever was stated.
//!
//! The address is `10.255.255.1`, which is routed (via the default route) and
//! answers nothing, so the SYN is dropped rather than refused. A host with no
//! default route refuses it at once, which proves nothing about the bound, so
//! that is an unmet precondition, not a pass.
//!
//! Run under nextest: each test opens a session from a one-slot pool, and the
//! forever case never returns, so it keeps its slot until the process exits.
//!
//! Run with:
//! cargo nextest run -p nros-rmw-zenoh --features platform-posix --test connect_budget

#![cfg(all(feature = "platform-posix", target_os = "linux"))]

// The platform layer the C shim calls into (`nros_platform_*`) is linked
// through the backend crate, as in `condvar_shim_codes`.
use nros_rmw_zenoh as _;
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

const LOCATOR: &core::ffi::CStr = c"tcp/10.255.255.1:7447";
/// `10.255.255.1:7447` as `/proc/net/tcp` prints a remote address.
const PROC_REMOTE: &str = "01FFFF0A:1D17";
/// The SYN-SENT state as `/proc/net/tcp` prints it in its state column.
const SYN_SENT: &str = "02";

fn require_default_route() {
    let routes = std::fs::read_to_string("/proc/net/route").unwrap_or_default();
    let has_default = routes
        .lines()
        .skip(1)
        .any(|l| l.split_whitespace().nth(1) == Some("00000000"));
    if !has_default {
        nros_tests::unmet!(
            "this host has no default route, so 10.255.255.1 is refused at once \
             instead of dropping the SYN; the connect bound cannot be observed"
        );
    }
}

/// Open a client session on [`LOCATOR`] under `budget_ms`; the open's return
/// code and how long it took.
fn open_with(budget_ms: i32) -> (i32, Duration) {
    // SAFETY: a fresh pool slot, initialised and opened in order, released on
    // the paths that return.
    unsafe {
        let handle = zpico_sys::zpico_session_acquire();
        assert!(
            !handle.is_null(),
            "no free zenoh session slot (run under nextest)"
        );
        assert_eq!(
            zpico_sys::zpico_init(handle, LOCATOR.as_ptr()),
            0,
            "zpico_init"
        );
        assert_eq!(
            zpico_sys::zpico_set_connect_timeout(handle, budget_ms),
            0,
            "zpico_set_connect_timeout({budget_ms})"
        );
        let start = Instant::now();
        let ret = zpico_sys::zpico_open(handle);
        let took = start.elapsed();
        zpico_sys::zpico_session_release(handle);
        (ret, took)
    }
}

/// The socket inodes this process holds (`/proc/self/fd` -> `socket:[N]`).
fn own_socket_inodes() -> BTreeSet<String> {
    std::fs::read_dir("/proc/self/fd")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_link(e.path()).ok())
        .filter_map(|l| {
            l.to_str()?
                .strip_prefix("socket:[")?
                .strip_suffix(']')
                .map(str::to_string)
        })
        .collect()
}

/// Local ports of THIS process's SYN_SENT sockets to [`LOCATOR`].
/// `/proc/net/tcp` is the whole host's, and the other tests here (separate
/// processes under nextest) dial the same address, so each row is kept only
/// when its inode is one of ours.
fn syn_sent_ports() -> BTreeSet<String> {
    let ours = own_socket_inodes();
    std::fs::read_to_string("/proc/net/tcp")
        .unwrap_or_default()
        .lines()
        .skip(1)
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (f.len() > 9 && f[2] == PROC_REMOTE && f[3] == SYN_SENT && ours.contains(f[9]))
                .then(|| f[1].rsplit(':').next().unwrap_or("").to_string())
        })
        .collect()
}

#[test]
fn one_attempt_gives_up_on_a_dropped_syn_at_once() {
    require_default_route();
    let (ret, took) = open_with(0);
    assert!(ret != 0, "an open of an unreachable address must fail");
    assert!(
        took < Duration::from_secs(3),
        "budget 0 is one attempt that fails at once; the open took {took:?}"
    );
}

#[test]
fn a_stated_budget_bounds_the_open() {
    require_default_route();
    let (ret, took) = open_with(3000);
    assert!(ret != 0, "an open of an unreachable address must fail");
    assert!(
        took < Duration::from_secs(6),
        "a 3 s budget must end the open near 3 s; it took {took:?}"
    );
}

/// Forever retries, so it never returns; what the fix changes is that each
/// ATTEMPT is bounded. Observed from outside: the attempt's SYN_SENT socket
/// goes away within the per-attempt bound (10 s) and a new attempt's socket,
/// on a new local port, takes its place. Unbounded, the first socket stays in
/// SYN_SENT for the host's whole retry limit.
#[test]
fn forever_bounds_each_attempt_and_retries() {
    require_default_route();
    std::thread::spawn(|| {
        let _ = open_with(-1);
    });
    let deadline = Instant::now() + Duration::from_secs(25);
    let mut seen = BTreeSet::new();
    while Instant::now() < deadline {
        seen.extend(syn_sent_ports());
        if seen.len() >= 2 {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        seen.len() >= 2,
        "forever must give up each attempt within its bound and retry on a new \
         socket; in 25 s it used only {seen:?}"
    );
}
