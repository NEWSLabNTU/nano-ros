//! RFC-0134 D2 -- two peer sessions on a point-to-point link, with no router.
//!
//! The link is IVC on its host mock: each end registers its half of a Unix
//! datagram socket pair with `nvidia_ivc::unix_mock::register_fd`, so the two
//! sessions talk to each other and to nothing else. One peer listens on
//! `ivc/1`, the other connects on `ivc/2`; a publisher on the connecting peer
//! reaches a subscriber on the listening one.
//!
//! The two peers are two PROCESSES: the test re-runs its own binary as the
//! listening peer. In one process the second session cannot open while the
//! first is still waiting in its handshake, so a single-process version could
//! only ever measure that, never the link.
//!
//! Before the fork patch this could not work at all: a peer session waited for
//! peers through a socket set and accepted them through a TCP/TLS accept loop,
//! and IVC has neither.
//!
//! Runs in `just check required-features-tests` (`.config/feature-lanes.toml`).
//! Needs no router and no network.

#![cfg(all(feature = "platform-posix", feature = "link-ivc"))]

use nros_rmw::{
    Publisher, QoSProfile, Session, SessionMode, Subscription, TopicInfo, Transport, TransportConfig,
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
const ROLE_ENV: &str = "NROS_P2P_TEST_ROLE";
const DIR_ENV: &str = "NROS_P2P_TEST_DIR";
const TEST_NAME: &str = "two_peers_on_a_point_to_point_link_exchange_data_without_a_router";

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
        assert!(Instant::now() < deadline, "the other end never bound {}", theirs.display());
        thread::sleep(Duration::from_millis(10));
    }
    sock.connect(theirs).expect("connect to the other end's socket");
    sock
}

/// The listening peer, run in the child process. Exits 0 when the message
/// arrives, non-zero otherwise.
fn listening_peer(dir: &Path) -> ! {
    nvidia_ivc::unix_mock::register_fd(1, socket_end(&dir.join("listen"), &dir.join("connect")));
    // The listener retries its handshake until this timeout, so the connecting
    // peer may start at any point inside it.
    let mut session = match ZenohTransport::open(&config(&[("listen", "ivc/1")])) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("listening peer did not open: {e:?}");
            std::process::exit(2);
        }
    };
    let mut subscriber = session
        .create_subscription(&topic(), QoSProfile::BEST_EFFORT)
        .expect("subscriber on the listening peer");
    let mut buf = [0u8; 64];
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(Some(len)) = subscriber.take_serialized(&mut buf) {
            let got = (len == 8).then(|| i32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]));
            std::process::exit(if got == Some(VALUE) { 0 } else { 3 });
        }
        thread::sleep(Duration::from_millis(20));
    }
    eprintln!("listening peer: no message within 15 s");
    std::process::exit(4);
}

/// A peer on `ivc/1` and a peer on `ivc/2`, in two processes, open with no
/// router, and a message published on one is delivered on the other.
#[test]
fn two_peers_on_a_point_to_point_link_exchange_data_without_a_router() {
    if std::env::var(ROLE_ENV).as_deref() == Ok("listener") {
        listening_peer(&PathBuf::from(std::env::var(DIR_ENV).unwrap()));
    }
    assert!(
        nros_rmw_zenoh::zpico::ZPICO_POINT_TO_POINT_PEER_SUPPORTED,
        "a build with the IVC link must compile point-to-point peer mode (RFC-0134 D1)"
    );

    let dir = std::env::temp_dir().join(format!("nros-p2p-peer-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST_NAME, "--nocapture", "--test-threads=1"])
        .env(ROLE_ENV, "listener")
        .env(DIR_ENV, &dir)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn the listening peer");

    nvidia_ivc::unix_mock::register_fd(2, socket_end(&dir.join("connect"), &dir.join("listen")));
    let mut session = ZenohTransport::open(&config(&[("connect", "ivc/2")]))
        .expect("the connecting peer should open over ivc/2 with no router");
    let publisher = session
        .create_publisher(&topic(), QoSProfile::BEST_EFFORT)
        .expect("publisher on the connecting peer");
    let mut msg = [0x00u8, 0x01, 0x00, 0x00, 0, 0, 0, 0];
    msg[4..].copy_from_slice(&VALUE.to_le_bytes());

    let deadline = Instant::now() + Duration::from_secs(20);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            break None;
        }
        publisher.publish_raw(&msg).expect("publish over the point-to-point link");
        thread::sleep(Duration::from_millis(200));
    };
    let _ = std::fs::remove_dir_all(&dir);
    let status = status.expect("the listening peer did not finish within 20 s");
    assert!(
        status.success(),
        "the listening peer did not receive the message ({status}); see its output above"
    );
}
