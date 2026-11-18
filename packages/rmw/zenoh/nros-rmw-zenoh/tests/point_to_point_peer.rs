//! RFC-0134 D2 -- two peer sessions on a point-to-point link, with no router.
//!
//! Each link runs on its host mock, over a Unix datagram socket pair: IVC
//! through `nvidia_ivc::unix_mock::register_fd`, RPMsg through
//! `zpico_link_rpmsg::unix_mock::register`, whose reader thread delivers into
//! the link's receive ring the way a board's RPMsg receive callback does. One
//! peer listens, the other connects; a message published on the connecting
//! peer, larger than one frame of either link, arrives intact on the
//! listening one.
//!
//! The two peers are two PROCESSES: the test re-runs its own binary as the
//! listening peer. In one process the second session cannot open while the
//! first is still waiting in its handshake, so a single-process version could
//! only ever measure that, never the link.
//!
//! Before the fork patch this could not work at all: a peer session waited for
//! peers through a socket set and accepted them through a TCP/TLS accept loop,
//! and neither link has either.
//!
//! Runs in `just check required-features-tests` (`.config/feature-lanes.toml`).
//! Needs no router and no network.

#![cfg(all(
    feature = "platform-posix",
    feature = "link-ivc",
    feature = "link-rpmsg"
))]

use nros_rmw::{
    Publisher, QoSProfile, Session, SessionMode, Subscription, TopicInfo, Transport,
    TransportConfig,
};
use nros_rmw_zenoh::ZenohTransport;
use std::{
    os::unix::net::UnixDatagram,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const VALUE: i32 = 4175;
/// Larger than one frame of either link (64 B on IVC, 496 B on RPMsg), so the
/// message is fragmented and reassembled; within the subscriber's default
/// 1024-byte receive buffer, which drops anything larger.
const PAYLOAD: usize = 1000;
const ROLE_ENV: &str = "NROS_P2P_TEST_ROLE";
const DIR_ENV: &str = "NROS_P2P_TEST_DIR";

/// A point-to-point link under test: how each end registers its socket with
/// the link's host mock, and the locator each end opens.
struct Link {
    test_name: &'static str,
    listen: &'static [(&'static str, &'static str)],
    connect: &'static [(&'static str, &'static str)],
    /// Register this end's socket; `listener` says which end.
    register: fn(listener: bool, sock: UnixDatagram),
}

fn config(properties: &'static [(&'static str, &'static str)]) -> TransportConfig<'static> {
    TransportConfig {
        locator: None,
        mode: SessionMode::Peer,
        properties,
        node_name: "",
        namespace: "",
        domain_id: 0,
    }
}

fn topic() -> TopicInfo<'static> {
    TopicInfo::new("test/point_to_point", "Int32", "hash2696")
}

/// Bind `mine` and connect it to `theirs` once `theirs` exists.
fn socket_end(mine: &Path, theirs: &Path) -> UnixDatagram {
    let sock = UnixDatagram::bind(mine).expect("bind this end's socket");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !theirs.exists() {
        assert!(
            Instant::now() < deadline,
            "the other end never bound {}",
            theirs.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
    sock.connect(theirs)
        .expect("connect to the other end's socket");
    sock
}

/// The listening peer, run in the child process. Exits 0 when the message
/// arrives, non-zero otherwise.
fn listening_peer(link: &Link, dir: &Path) -> ! {
    (link.register)(true, socket_end(&dir.join("listen"), &dir.join("connect")));
    let mut session = match ZenohTransport::open(&config(link.listen)) {
        Ok(s) => s,
        Err(e) => panic!("listening peer did not open: {e:?}"),
    };
    let mut subscriber = session
        .create_subscription(&topic(), QoSProfile::BEST_EFFORT)
        .expect("subscriber on the listening peer");
    let mut buf = [0u8; 2048];
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(Some(len)) = subscriber.take_serialized(&mut buf) {
            let intact = len == PAYLOAD
                && i32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) == VALUE
                && buf[8..len].iter().enumerate().all(|(i, b)| *b == i as u8);
            if !intact {
                eprintln!("listening peer: {len} bytes arrived, not the {PAYLOAD} sent");
            }
            std::process::exit(if intact { 0 } else { 3 });
        }
        thread::sleep(Duration::from_millis(20));
    }
    eprintln!("listening peer: no message within 15 s");
    std::process::exit(4);
}

/// A peer on each end of `link`, in two processes, open with no router, and a
/// message published on one is delivered on the other.
fn exchange_without_a_router(link: &Link) {
    if std::env::var(ROLE_ENV).as_deref() == Ok("listener") {
        listening_peer(link, &PathBuf::from(std::env::var(DIR_ENV).unwrap()));
    }
    // A const: the build itself refuses an image that lacks peer mode.
    const {
        assert!(
            nros_rmw_zenoh::zpico::ZPICO_POINT_TO_POINT_PEER_SUPPORTED,
            "a build with a point-to-point link must compile point-to-point peer mode (RFC-0134 D1)"
        )
    };

    let dir = std::env::temp_dir().join(format!(
        "nros-p2p-peer-{}-{}",
        link.test_name.len(),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", link.test_name, "--nocapture", "--test-threads=1"])
        .env(ROLE_ENV, "listener")
        .env(DIR_ENV, &dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn the listening peer");

    (link.register)(false, socket_end(&dir.join("connect"), &dir.join("listen")));
    let mut session = ZenohTransport::open(&config(link.connect))
        .expect("the connecting peer should open with no router");
    let publisher = session
        .create_publisher(&topic(), QoSProfile::BEST_EFFORT)
        .expect("publisher on the connecting peer");
    let mut msg = [0u8; PAYLOAD];
    msg[..4].copy_from_slice(&[0x00, 0x01, 0x00, 0x00]);
    msg[4..8].copy_from_slice(&VALUE.to_le_bytes());
    for (i, b) in msg[8..].iter_mut().enumerate() {
        *b = i as u8;
    }

    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break None;
        }
        publisher
            .publish_raw(&msg)
            .expect("publish over the point-to-point link");
        thread::sleep(Duration::from_millis(200));
    };
    let _ = std::fs::remove_dir_all(&dir);
    let status = status.expect("the listening peer did not finish within 20 s");
    assert!(
        status.success(),
        "the listening peer did not receive the message ({status}); see its output above"
    );
}

#[test]
fn two_peers_on_an_ivc_link_exchange_data_without_a_router() {
    exchange_without_a_router(&Link {
        test_name: "two_peers_on_an_ivc_link_exchange_data_without_a_router",
        listen: &[("listen", "ivc/1")],
        connect: &[("connect", "ivc/2")],
        register: |listener, sock| {
            nvidia_ivc::unix_mock::register_fd(if listener { 1 } else { 2 }, sock)
        },
    });
}

#[test]
fn two_peers_on_an_rpmsg_link_exchange_data_without_a_router() {
    exchange_without_a_router(&Link {
        test_name: "two_peers_on_an_rpmsg_link_exchange_data_without_a_router",
        listen: &[("listen", "rpmsg/nros-a")],
        connect: &[("connect", "rpmsg/nros-b")],
        register: |listener, sock| {
            zpico_link_rpmsg::unix_mock::register(if listener { "nros-a" } else { "nros-b" }, sock)
        },
    });
}
