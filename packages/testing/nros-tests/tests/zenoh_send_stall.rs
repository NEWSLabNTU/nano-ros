//! A zenoh session whose router stops reading ends in a visible close
//! (issue 2389).
//!
//! A client's TCP write to a router that keeps the socket open but stops
//! reading used to block for ever: zenoh-pico's link write was a plain blocking
//! `send()`, so once the window and both socket buffers were full the writer
//! sat in it holding the transport's TX mutex. Every other sender queued
//! behind that mutex (publishers, keep-alives, and the close the lease task
//! would send), so the session never recovered and never said why. On Zephyr
//! the blocked send also held the socket's per-fd lock, so the read task could
//! not even receive. Measured before the fix, on Linux and on native_sim: the
//! image published until the buffers filled, then printed nothing more for as
//! long as it ran (90 s), with `send()` at the top of the writer's stack.
//!
//! The fix bounds the write: a stream link's write fails once the socket has
//! taken no byte for `Z_TRANSPORT_TX_WAIT_BEFORE_CLOSE` (5 s, zenoh's own
//! "wait before close" setting), the link is marked broken (part of a frame may be on
//! the wire), zenoh-pico's read task closes the session, and the zpico shim
//! prints [`ZPICO_LINK_TX_CLOSED_PREFIX`].
//!
//! The test stands a proxy between the talker and a real router. The proxy
//! relays both directions until the talker has sent [`RELAY_BEFORE_STOP`]
//! bytes (the handshake, the declarations and some data), then stops reading
//! from the talker while keeping both sockets open. The talker publishes
//! `RELIABLE` (congestion control BLOCK) in a tight loop. The line must appear
//! no sooner than the stall bound after the proxy stopped reading, and within
//! [`DEADLINE`].
//!
//! ## Prerequisites
//!
//! - the `packages/testing/nros-bench/stress-zenoh` fixture row
//! - `rmw_zenohd` (ROS 2's zenoh router)
//!
//! Run with: `cargo nextest run -p nros-tests --test zenoh_send_stall`

use nros_tests::{
    fixtures::{
        ManagedProcess, ZenohRouter, require_zenohd, zenoh_stress_test_binary, zenohd_unique,
    },
    output::ZPICO_LINK_TX_CLOSED_PREFIX,
};
use rstest::rstest;
use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::PathBuf,
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// What the proxy relays from the talker before it stops reading: past the
/// session handshake and the declarations, into the data.
const RELAY_BEFORE_STOP: usize = 256 * 1024;

/// zenoh-pico's `Z_TRANSPORT_TX_WAIT_BEFORE_CLOSE` (the fork's `config.h.in`
/// and `nros-zpico-build`'s generated header): how long a write may take no
/// byte. The close cannot come sooner than this after the proxy stops reading.
const STALL_BOUND: Duration = Duration::from_millis(5000);

/// From the proxy's stop to the line: filling the loopback socket buffers
/// (several MiB, autotuned), the stall bound, and the read task's next turn.
/// Measured 5.2 s on Linux (the buffers fill in well under a second at this rate); without the bound the line never comes.
const DEADLINE: Duration = Duration::from_secs(60);

/// A relay that stops reading from the client after `RELAY_BEFORE_STOP`
/// bytes. Every connection it accepts is treated the same way, so a talker
/// that reconnects stalls again.
struct StopReadingProxy {
    port: u16,
    stopped_at: Arc<Mutex<Option<Instant>>>,
    quit: Arc<AtomicBool>,
}

impl StopReadingProxy {
    fn start(router_port: u16) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("proxy: bind");
        let port = listener.local_addr().expect("proxy: local addr").port();
        listener
            .set_nonblocking(true)
            .expect("proxy: non-blocking accept");
        let stopped_at = Arc::new(Mutex::new(None));
        let quit = Arc::new(AtomicBool::new(false));
        let (stop, q) = (stopped_at.clone(), quit.clone());
        thread::spawn(move || {
            while !q.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((client, _)) => {
                        client
                            .set_nonblocking(false)
                            .expect("proxy: blocking client");
                        let router = TcpStream::connect(("127.0.0.1", router_port))
                            .expect("proxy: connect to the router");
                        Self::relay(client, router, stop.clone(), q.clone());
                    }
                    Err(_) => thread::sleep(Duration::from_millis(20)),
                }
            }
        });
        StopReadingProxy {
            port,
            stopped_at,
            quit,
        }
    }

    fn relay(
        client: TcpStream,
        router: TcpStream,
        stopped_at: Arc<Mutex<Option<Instant>>>,
        quit: Arc<AtomicBool>,
    ) {
        // router -> client: relayed for as long as the router sends. When the
        // router hangs up, the client side is NOT closed: the talker must see
        // a peer that is there and does not read, never an EOF.
        let (mut from_router, mut to_client) = (
            router.try_clone().expect("proxy: clone"),
            client.try_clone().expect("proxy: clone"),
        );
        thread::spawn(move || {
            let mut buf = [0u8; 16 * 1024];
            while let Ok(n) = from_router.read(&mut buf) {
                if n == 0 || to_client.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
        });
        // client -> router: relayed until RELAY_BEFORE_STOP, then never read
        // again. The sockets live until the test ends.
        thread::spawn(move || {
            let (mut from_client, mut to_router) = (client, router);
            let mut buf = [0u8; 16 * 1024];
            let mut relayed = 0usize;
            while relayed < RELAY_BEFORE_STOP {
                match from_client.read(&mut buf) {
                    Ok(0) | Err(_) => return,
                    Ok(n) => {
                        if to_router.write_all(&buf[..n]).is_err() {
                            return;
                        }
                        relayed += n;
                    }
                }
            }
            stopped_at.lock().unwrap().get_or_insert_with(Instant::now);
            while !quit.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(50));
            }
            let _ = from_client.shutdown(Shutdown::Both);
        });
    }

    fn stopped_at(&self) -> Option<Instant> {
        *self.stopped_at.lock().unwrap()
    }
}

impl Drop for StopReadingProxy {
    fn drop(&mut self) {
        self.quit.store(true, Ordering::Relaxed);
    }
}

#[rstest]
fn zenoh_send_stall_closes_the_session_visibly(
    zenohd_unique: ZenohRouter,
    zenoh_stress_test_binary: PathBuf,
) {
    require_zenohd();
    let router_locator = zenohd_unique.locator();

    // A subscriber on the router: with none, zenoh-pico's write filter drops
    // every put before it reaches the link, and nothing ever fills.
    let mut listener_cmd = Command::new(&zenoh_stress_test_binary);
    listener_cmd
        .env("NROS_LOCATOR", &router_locator)
        .env("MODE", "listener")
        .env("PAYLOAD_SIZE", "1024")
        .env("EXPECTED_COUNT", "100000000")
        .env("TIMEOUT_SECS", "120");
    let mut listener = ManagedProcess::spawn_image(listener_cmd, "send-stall-listener")
        .expect("start the stress listener");
    listener
        .wait_for_output_pattern("Ready: listening", Duration::from_secs(10))
        .expect("the stress listener did not become ready");

    let proxy = StopReadingProxy::start(zenohd_unique.port());

    let mut talker_cmd = Command::new(&zenoh_stress_test_binary);
    talker_cmd
        .env("NROS_LOCATOR", format!("tcp/127.0.0.1:{}", proxy.port))
        .env("MODE", "talker")
        .env("PAYLOAD_SIZE", "1024")
        .env("PUBLISH_COUNT", "2000000")
        .env("PUBLISH_INTERVAL_MS", "0");
    let mut talker =
        ManagedProcess::spawn_image(talker_cmd, "send-stall-talker").expect("start the talker");

    let output = talker.collect_until(
        ZPICO_LINK_TX_CLOSED_PREFIX,
        DEADLINE + Duration::from_secs(15),
    );
    let seen_at = Instant::now();
    talker.kill();
    listener.kill();

    let stopped_at = proxy.stopped_at().unwrap_or_else(|| {
        panic!(
            "the proxy never relayed {RELAY_BEFORE_STOP} bytes from the talker; output:\n{}",
            tail(&output)
        )
    });
    assert!(
        output.contains(ZPICO_LINK_TX_CLOSED_PREFIX),
        "the router stopped reading {:?} ago and the talker never said its session closed \
         (a write that blocks for ever is issue 2389). Output tail:\n{}",
        seen_at.duration_since(stopped_at),
        tail(&output),
    );
    let after = seen_at.duration_since(stopped_at);
    assert!(
        after >= STALL_BOUND,
        "the session closed {after:?} after the router stopped reading, sooner than the \
         {STALL_BOUND:?} a write may wait for room. Output tail:\n{}",
        tail(&output),
    );
    assert!(
        after <= DEADLINE,
        "the session closed {after:?} after the router stopped reading; the bound is \
         {DEADLINE:?}. Output tail:\n{}",
        tail(&output),
    );
    eprintln!("send stall: session closed {after:?} after the router stopped reading");
}

/// The last lines of a long output: the talker prints a line per publish.
fn tail(output: &str) -> String {
    let lines: Vec<&str> = output.lines().collect();
    lines[lines.len().saturating_sub(40)..].join("\n")
}
