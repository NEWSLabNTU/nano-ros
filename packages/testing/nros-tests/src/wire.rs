//! A userspace Ethernet wire between a QEMU guest NIC and QEMU's user network,
//! serialised at the line rate the emulated PHY advertises (issue 0917).
//!
//! QEMU hands a guest NIC each frame as fast as its backend produces it: a
//! burst that takes ~1 ms on a 100 Mbps wire lands in the emulated NIC at
//! once, and the patched LAN9118 model BACK-PRESSURES a full RX FIFO (it
//! stops reading its backend) where the part drops the frame. So on stock
//! QEMU no drain latency is ever punished: a guest that sleeps 5 ms before
//! looking at a 6-frame FIFO loses nothing. This wire puts both properties of
//! the real link back, unprivileged (no tap, no `tc`):
//!
//! * **Pacing.** Each direction is a FIFO that releases frames back to back at
//!   [`WireConfig::rate_bps`], counting preamble + SFD (8 B), FCS (4 B), the
//!   inter-frame gap (12 B) and the 60-byte minimum.
//! * **Overflow.** Host to guest, a frame that falls due while QEMU has left
//!   an earlier frame unread for longer than [`WireConfig::stall`] is DROPPED,
//!   as the part would drop it. Unread means QEMU stopped reading its backend,
//!   which it does only while the NIC refuses frames. QEMU still holds the one
//!   frame it was refused, and one more waits unread, so the lane holds about
//!   two frames more than the NIC's FIFO: kinder than silicon, never harsher.
//!
//! Topology: both ends are QEMU `-netdev dgram` on `AF_UNIX` datagram sockets.
//! The guest NIC's netdev peers with this wire; the wire's other end is a
//! second `dgram` netdev on a hub with `-netdev user` (slirp), so host
//! processes reach the guest through slirp's `hostfwd` exactly as on the
//! plain user network. [`WireLink::qemu_args`] renders the QEMU words.
//!
//! "Unread" is read from `SIOCOUTQ` on the wire's guest-side socket: for a
//! Unix datagram socket it is the truesize of the datagrams the peer has not
//! received yet. No busy-wait: the thread sleeps in `ppoll` until the next
//! frame is due or a frame arrives, so it never competes with QEMU's main loop
//! for a CPU (a spinning relay that shares a core with QEMU's main thread
//! delays QEMU's own reads by milliseconds and reads as NIC overflow).

use std::{
    collections::VecDeque,
    io,
    os::{fd::AsRawFd, unix::net::UnixDatagram},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// How the wire behaves.
#[derive(Debug, Clone, Copy)]
pub struct WireConfig {
    /// Line rate in bits per second.
    pub rate_bps: f64,
    /// Drop a host-to-guest frame that falls due while an earlier one has been
    /// unread this long. `None` keeps QEMU's back-pressure (no loss).
    pub stall: Option<Duration>,
}

impl WireConfig {
    /// 100BASE-TX, the LAN9118's PHY, with the NIC's overflow modelled.
    #[must_use]
    pub const fn fast_ethernet() -> Self {
        Self {
            rate_bps: 100e6,
            stall: Some(Duration::from_micros(100)),
        }
    }
}

/// What crossed the wire.
#[derive(Debug, Clone, Copy, Default)]
pub struct WireStats {
    /// Host-to-guest frames delivered to QEMU.
    pub to_guest: u64,
    /// Host-to-guest frames dropped because the NIC was refusing frames.
    pub to_guest_dropped: u64,
    /// Guest-to-host frames delivered to slirp.
    pub to_host: u64,
}

/// A running wire. Dropping it stops the thread.
pub struct WireLink {
    dir: tempfile::TempDir,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<WireStats>>,
}

const Q_GUEST: &str = "q_guest";
const R_GUEST: &str = "r_guest";
const Q_HOST: &str = "q_host";
const R_HOST: &str = "r_host";

impl WireLink {
    /// Bind both ends and start the wire thread.
    ///
    /// # Errors
    ///
    /// The socket directory or a socket cannot be created.
    pub fn start(cfg: WireConfig) -> io::Result<Self> {
        // A short path: `sun_path` holds 108 bytes.
        let dir = tempfile::Builder::new()
            .prefix("nros-wire")
            .tempdir_in("/tmp")?;
        let g = UnixDatagram::bind(dir.path().join(R_GUEST))?;
        let h = UnixDatagram::bind(dir.path().join(R_HOST))?;
        g.set_nonblocking(true)?;
        h.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            let q_guest = dir.path().join(Q_GUEST);
            let q_host = dir.path().join(Q_HOST);
            std::thread::Builder::new()
                .name("nros-wire".into())
                .spawn(move || run(&cfg, &g, &h, &q_guest, &q_host, &stop))?
        };
        Ok(Self {
            dir,
            stop,
            thread: Some(thread),
        })
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// The QEMU words that put the guest's on-board NIC (`model`) on this wire
    /// and the wire's far end on slirp with `user_opts` (e.g. `hostfwd=...`).
    #[must_use]
    pub fn qemu_args(&self, model: &str, user_opts: &str) -> Vec<String> {
        let dgram = |id: &str, local: &str, remote: &str| {
            format!(
                "dgram,id={id},local.type=unix,local.path={},remote.type=unix,remote.path={}",
                self.path(local).display(),
                self.path(remote).display()
            )
        };
        let user = if user_opts.is_empty() {
            "user,id=nros_wire_user".to_string()
        } else {
            format!("user,id=nros_wire_user,{user_opts}")
        };
        [
            "-netdev",
            &dgram("nros_wire_guest", Q_GUEST, R_GUEST),
            "-net",
            &format!("nic,model={model},netdev=nros_wire_guest"),
            "-netdev",
            &user,
            "-netdev",
            &dgram("nros_wire_host", Q_HOST, R_HOST),
            "-netdev",
            "hubport,id=nros_wire_p0,hubid=17,netdev=nros_wire_user",
            "-netdev",
            "hubport,id=nros_wire_p1,hubid=17,netdev=nros_wire_host",
        ]
        .iter()
        .map(|s| (*s).to_string())
        .collect()
    }

    /// Stop the wire and return what crossed it.
    #[must_use]
    pub fn stop(mut self) -> WireStats {
        self.stop.store(true, Ordering::Relaxed);
        self.thread
            .take()
            .and_then(|t| t.join().ok())
            .unwrap_or_default()
    }
}

impl Drop for WireLink {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Bytes the peer has not read yet (`SIOCOUTQ`).
fn outq(s: &UnixDatagram) -> usize {
    let mut v: libc::c_int = 0;
    // SAFETY: TIOCOUTQ (== SIOCOUTQ) writes one c_int through the pointer.
    let rc = unsafe { libc::ioctl(s.as_raw_fd(), libc::TIOCOUTQ, &mut v) };
    if rc == 0 {
        usize::try_from(v).unwrap_or(0)
    } else {
        0
    }
}

fn wire_time(cfg: &WireConfig, len: usize) -> Duration {
    // preamble+SFD 8, FCS 4, IFG 12; 60-byte minimum before the FCS.
    let bits = (len.max(60) + 24) as f64 * 8.0;
    Duration::from_secs_f64(bits / cfg.rate_bps)
}

struct Lane {
    queue: VecDeque<(Instant, Vec<u8>)>,
    wire_free: Instant,
}

impl Lane {
    fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            wire_free: Instant::now(),
        }
    }

    fn push(&mut self, cfg: &WireConfig, now: Instant, frame: Vec<u8>) {
        self.wire_free = self.wire_free.max(now) + wire_time(cfg, frame.len());
        self.queue.push_back((self.wire_free, frame));
    }
}

fn run(
    cfg: &WireConfig,
    g: &UnixDatagram,
    h: &UnixDatagram,
    q_guest: &Path,
    q_host: &Path,
    stop: &AtomicBool,
) -> WireStats {
    let mut st = WireStats::default();
    let mut to_guest = Lane::new();
    let mut to_host = Lane::new();
    // (sent at, truesize) of host-to-guest frames QEMU has not read, oldest first.
    let mut unread: VecDeque<(Instant, usize)> = VecDeque::new();
    let mut unread_bytes = 0usize;
    let mut buf = vec![0u8; 65_536];
    while !stop.load(Ordering::Relaxed) {
        let now = Instant::now();
        // Frames due now.
        while to_guest.queue.front().is_some_and(|(t, _)| *t <= now) {
            let (_, frame) = to_guest.queue.pop_front().expect("front checked");
            let oq = outq(g);
            while unread_bytes > oq {
                let Some((_, ts)) = unread.pop_front() else {
                    break;
                };
                unread_bytes -= ts;
            }
            if oq == 0 {
                unread.clear();
                unread_bytes = 0;
            }
            if let (Some(stall), Some((sent, _))) = (cfg.stall, unread.front())
                && now.duration_since(*sent) > stall
            {
                st.to_guest_dropped += 1;
                continue;
            }
            if g.send_to(&frame, q_guest).is_ok() {
                st.to_guest += 1;
                let ts = outq(g).saturating_sub(oq);
                if ts > 0 {
                    unread.push_back((now, ts));
                    unread_bytes += ts;
                }
            }
        }
        while to_host.queue.front().is_some_and(|(t, _)| *t <= now) {
            let (_, frame) = to_host.queue.pop_front().expect("front checked");
            if h.send_to(&frame, q_host).is_ok() {
                st.to_host += 1;
            }
        }
        // Sleep until the next frame is due or one arrives (10 ms at most, so
        // `stop` is seen).
        let next = [to_guest.queue.front(), to_host.queue.front()]
            .into_iter()
            .flatten()
            .map(|(t, _)| *t)
            .min();
        let wait = next.map_or(Duration::from_millis(10), |t| {
            t.saturating_duration_since(Instant::now())
        });
        let mut fds = [
            libc::pollfd {
                fd: g.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: h.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let ts = libc::timespec {
            tv_sec: libc::time_t::try_from(wait.as_secs()).unwrap_or(0),
            tv_nsec: libc::c_long::from(wait.subsec_nanos()),
        };
        // SAFETY: two valid pollfds, a valid timespec, no signal mask.
        unsafe { libc::ppoll(fds.as_mut_ptr(), 2, &ts, std::ptr::null()) };
        let now = Instant::now();
        while let Ok(n) = h.recv(&mut buf) {
            to_guest.push(cfg, now, buf[..n].to_vec());
        }
        while let Ok(n) = g.recv(&mut buf) {
            to_host.push(cfg, now, buf[..n].to_vec());
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_frame_takes_its_wire_time_at_100_mbps() {
        let cfg = WireConfig::fast_ethernet();
        // 1514 B frame + 24 B of framing = 12 304 bits = 123.04 us.
        let t = wire_time(&cfg, 1514);
        assert!((t.as_secs_f64() - 123.04e-6).abs() < 1e-8, "{t:?}");
        // A runt is padded to the 60-byte minimum.
        assert_eq!(wire_time(&cfg, 10), wire_time(&cfg, 60));
    }

    #[test]
    fn a_burst_leaves_back_to_back_at_line_rate() {
        let cfg = WireConfig::fast_ethernet();
        let mut lane = Lane::new();
        let t0 = Instant::now() + Duration::from_millis(5);
        for _ in 0..8 {
            lane.push(&cfg, t0, vec![0; 1514]);
        }
        let last = lane.queue.back().expect("pushed").0;
        let span = last.duration_since(t0).as_secs_f64();
        assert!((span - 8.0 * 123.04e-6).abs() < 1e-6, "{span}");
    }
}
