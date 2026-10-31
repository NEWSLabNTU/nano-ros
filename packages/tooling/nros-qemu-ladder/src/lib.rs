//! The ONE QEMU ladder (issue 1851, phase-517 W14): which `qemu-system-<arch>`
//! runs an image, and the argument line each QEMU board boots under.
//!
//! Two consumers ask these questions and used to answer them separately: the
//! `nros` CLI (`nros_cli_core::orchestration::qemu_bin`, and the commands
//! `nros build` prints in `cmd::build_next`) and the test harness
//! (`nros_tests::qemu::qemu_system_path` and the `QemuProcess::start_*`
//! launchers). The CLI's NuttX arm `virt`, NuttX rv-virt and ThreadX riscv64
//! lines were copied out of the harness (phase-517 W6), and the copies had
//! already drifted in argument order and in the ThreadX MAC. Both now call
//! this crate; the shell twin is `nros_qemu_bin` (`scripts/lib/qemu-bin.sh`).
//!
//! # The ladder
//!
//!   1. `QEMU_SYSTEM_<ARCH>` — the developer override / CI pin;
//!   2. a checkout's project prefix `<build root>/qemu/` (made by
//!      `just qemu setup-qemu`), only when its `.nros-provenance` names the
//!      version the index pins (issue 0930);
//!   3. the `nros setup` store at the PINNED `[tool.qemu]` version.
//!
//! What happens past rung 3 is the CONSUMER's, and the two differ on purpose:
//!
//! * the CLI has no `$PATH` rung — it prints a command for a person to paste,
//!   and a `qemu-system-arm` from `$PATH` is the distro build without the
//!   LAN9118 flow-control patch the store carries (issues 0830/0917), so it
//!   reports [`QemuBin::Missing`] with the remedy;
//! * the test harness falls back to the bare name (`$PATH`), so a host that
//!   never ran setup produces the documented precondition report rather than
//!   an exec error.
//!
//! A target the store does not BUILD is answered [`QemuBin::Host`] before
//! rungs 2–3 (the store's target list is a fact of the index:
//! `[tool.qemu.source] configure`'s `--target-list`). A consumer with no index
//! to read passes `store_targets: None`, and every target is then assumed to
//! be the store's — which, for the harness, ends at `$PATH` either way.
//!
//! A STALE project prefix (its provenance names another version) is skipped
//! by the CLI, which says so in the note, and refused by the harness, which
//! must not run a test on an emulator nobody chose: [`StalePrefix`].

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Where the QEMU came from, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QemuBin {
    /// A rung answered.
    Found {
        path: PathBuf,
        /// Which rung (`env`, `project`, `store`) — for `--why`.
        rung: &'static str,
    },
    /// No rung answered; `why` says what was looked at and how to fix it.
    Missing { why: String },
    /// The store builds no QEMU for this target, so it is the host's own
    /// (resolved from `$PATH` when the command runs); `why` says so and where
    /// to install it.
    Host { why: String },
    /// The project prefix holds this binary but is not the pinned QEMU, and
    /// the caller asked for that to be refused ([`StalePrefix::Refuse`]).
    RefusedPrefix { path: PathBuf, why: String },
}

/// What a stale project prefix (rung 2) does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StalePrefix {
    /// Fall through to the store; the reason joins a later `Missing` note.
    Skip,
    /// Stop: answer [`QemuBin::RefusedPrefix`].
    Refuse,
}

/// The inputs the ladder reads, gathered by the caller so the ladder itself is
/// testable without an environment, a checkout or a store.
pub struct Ctx<'a> {
    /// Environment lookup (rung 1).
    pub getenv: &'a dyn Fn(&str) -> Option<OsString>,
    /// The checkout's QEMU project prefix, `<build root>/qemu` (rung 2);
    /// `None` when there is no checkout, or for a QEMU the prefix never holds
    /// (the Espressif fork).
    pub project_prefix: Option<PathBuf>,
    /// The version the tool's index row pins, when an index was readable.
    pub pin: Option<String>,
    /// The store's pinned tool ROOT, `<store>/sdk/<tool>/<pin>` (rung 3).
    pub store_dir: Option<PathBuf>,
    /// The `qemu-system-<arch>` arches the store's QEMU is configured for
    /// (`arm`, `riscv64`), read from the index; `None` when no index was
    /// readable (every target is then assumed to be the store's).
    pub store_targets: Option<Vec<String>>,
    /// What a stale project prefix does.
    pub stale_prefix: StalePrefix,
}

/// `QEMU_SYSTEM_ARM` for `qemu-system-arm` — the override's variable name,
/// shared with `nros_qemu_bin`.
#[must_use]
pub fn env_var(exe: &str) -> String {
    let arch = exe.strip_prefix("qemu-system-").unwrap_or(exe);
    format!(
        "QEMU_SYSTEM_{}",
        arch.to_ascii_uppercase().replace('-', "_")
    )
}

/// The `-softmmu` arches a QEMU `./configure` line's `--target-list` names.
#[must_use]
pub fn targets_of_configure(configure: &str) -> Option<Vec<String>> {
    let list = configure
        .split_whitespace()
        .find_map(|w| w.strip_prefix("--target-list="))?;
    Some(
        list.split(',')
            .filter_map(|t| t.strip_suffix("-softmmu"))
            .map(str::to_string)
            .collect(),
    )
}

/// The distro package that carries `exe`, for the install hint.
#[must_use]
pub fn distro_hint(exe: &str) -> &'static str {
    match exe {
        "qemu-system-arm" => "qemu-system-arm",
        "qemu-system-x86_64" | "qemu-system-i386" => "qemu-system-x86",
        _ => "qemu-system-misc",
    }
}

/// Resolve `exe` (`qemu-system-arm`, `qemu-system-riscv64`, …).
#[must_use]
pub fn locate(exe: &str, ctx: &Ctx<'_>) -> QemuBin {
    let var = env_var(exe);
    if let Some(v) = (ctx.getenv)(&var).filter(|v| !v.is_empty()) {
        return QemuBin::Found {
            path: PathBuf::from(v),
            rung: "env",
        };
    }
    let arch = exe.strip_prefix("qemu-system-").unwrap_or(exe);
    if let Some(targets) = &ctx.store_targets
        && !targets.iter().any(|t| t == arch)
    {
        return QemuBin::Host {
            why: format!(
                "the nros store's QEMU is built for {} only, so `{exe}` is your host's: install \
                 it from your distribution (Debian/Ubuntu: `apt install {}`) or point ${var} \
                 at one",
                targets.join(", "),
                distro_hint(exe)
            ),
        };
    }
    let mut stale = None;
    if let Some(prefix) = &ctx.project_prefix {
        let cand = prefix.join("bin").join(exe);
        if cand.is_file() {
            match prefix_matches_pin(prefix, ctx.pin.as_deref()) {
                Ok(()) => {
                    return QemuBin::Found {
                        path: cand,
                        rung: "project",
                    };
                }
                Err(why) if ctx.stale_prefix == StalePrefix::Refuse => {
                    return QemuBin::RefusedPrefix { path: cand, why };
                }
                Err(why) => {
                    stale = Some(format!("{} is not the pinned QEMU: {why}", cand.display()))
                }
            }
        }
    }
    if let Some(dir) = &ctx.store_dir {
        let cand = dir.join("bin").join(exe);
        if cand.is_file() {
            return QemuBin::Found {
                path: cand,
                rung: "store",
            };
        }
    }
    let mut why = format!(
        "no {exe} from ${var}, the project prefix or the store{} — provision it with \
         `nros setup --tool qemu`",
        ctx.pin
            .as_deref()
            .map(|p| format!(" (pinned {p})"))
            .unwrap_or_default()
    );
    if let Some(s) = stale {
        why = format!("{s}; {why}");
    }
    QemuBin::Missing { why }
}

/// Issue 0930 — the project prefix is ONE directory across pin bumps, so it
/// counts only when its provenance marker names the pinned version. With no
/// pin to compare against, it is taken as chosen.
pub fn prefix_matches_pin(prefix: &Path, pin: Option<&str>) -> Result<(), String> {
    let Some(pin) = pin else {
        return Ok(());
    };
    let marker = prefix.join(".nros-provenance");
    let Ok(text) = std::fs::read_to_string(&marker) else {
        return Err(format!(
            "it has no {} (it was not installed by `nros setup`), so its version is \
             unknown; the index pins {pin}",
            marker.display()
        ));
    };
    let held = text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "version").then(|| v.trim().trim_matches('"').to_string())
    });
    match held {
        Some(v) if v == pin => Ok(()),
        Some(v) => Err(format!("it holds {v}; the index pins {pin}")),
        None => Err(format!(
            "{} names no version; the index pins {pin}",
            marker.display()
        )),
    }
}

// =============================================================================
// Per-board argument lines
// =============================================================================

/// A QEMU board whose image has no build-system runner, so its boot line is
/// ours to state (the platforms `nros_tests::qemu` launches by hand, RFC-0051
/// §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Board {
    /// NuttX on `qemu-system-arm -M virt -cpu cortex-a7`.
    NuttxArmVirt,
    /// NuttX rv-virt on `qemu-system-riscv32 -M virt`.
    NuttxRvVirt,
    /// ThreadX on `qemu-system-riscv64 -M virt`.
    ThreadxRiscv64,
}

/// How the board's NIC is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Net<'a> {
    /// QEMU user-mode (slirp) networking on a virtio-net MMIO device; `mac`
    /// pins the device's address when the firmware expects one.
    User { mac: Option<&'a str> },
    /// A point-to-point AF_UNIX datagram link to ONE sibling QEMU
    /// (`-netdev dgram,local.type=unix,…`, QEMU 7.2+): frames sent to `remote`
    /// arrive at the peer's `local`, with no root, routes or IGMP. The test
    /// harness's two-guest pairs use it; the CLI never prints it. Issue 2170:
    /// the pair launchers restated their board's machine line by hand until
    /// this existed.
    Dgram {
        local: &'a str,
        remote: &'a str,
        mac: &'a str,
    },
    /// No NIC at all (boot-only images).
    None,
}

/// One board's boot line around its kernel image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The `qemu-system-<arch>` this board needs — resolve it with [`locate`].
    pub exe: &'static str,
    /// Arguments up to and including `-kernel`.
    pub before_kernel: Vec<String>,
    /// Arguments after the kernel image.
    pub after_kernel: Vec<String>,
}

impl Line {
    /// The whole argument vector (without the executable) for `kernel`.
    #[must_use]
    pub fn argv(&self, kernel: &str) -> Vec<String> {
        let mut v = self.before_kernel.clone();
        v.push(kernel.to_string());
        v.extend(self.after_kernel.iter().cloned());
        v
    }
}

/// The MAC a ThreadX riscv64 image expects for `peer_index` — `52:54:00:12:34:56`
/// for 0 (talker/server), `…:57` for 1 (listener/client). It must match the
/// firmware's `Config::default()` / `Config::listener()` presets in
/// `nros-board-threadx-qemu-riscv64`; peer 0's is also QEMU's own default for
/// a first NIC.
#[must_use]
pub fn threadx_riscv64_mac(peer_index: u8) -> String {
    format!("52:54:00:12:34:{:02x}", 0x56u8.wrapping_add(peer_index))
}

fn words(s: &[&str]) -> Vec<String> {
    s.iter().map(|w| (*w).to_string()).collect()
}

/// The boot line for `board` with its NIC attached per `net`.
///
/// NuttX's defconfigs enable `CONFIG_DRIVERS_VIRTIO_MMIO`, so on both NuttX
/// boards the NIC is a `virtio-net-device` on the MMIO transport, never the
/// PCI device `-nic user` would add on `virt`. `-icount shift=auto` keeps
/// NuttX's virtual clock on wall-clock so `sleep()` and timeouts run at real
/// rates. ThreadX riscv64 needs the non-legacy virtio-mmio transport and its
/// NIC on the first MMIO bus.
#[must_use]
pub fn board_line(board: Board, net: Net<'_>) -> Line {
    let (exe, before) = match board {
        Board::NuttxArmVirt => (
            "qemu-system-arm",
            words(&[
                "-M",
                "virt",
                "-cpu",
                "cortex-a7",
                "-nographic",
                "-icount",
                "shift=auto",
                "-kernel",
            ]),
        ),
        Board::NuttxRvVirt => (
            "qemu-system-riscv32",
            words(&[
                "-M",
                "virt",
                "-bios",
                "none",
                "-nographic",
                "-icount",
                "shift=auto",
                "-kernel",
            ]),
        ),
        Board::ThreadxRiscv64 => (
            "qemu-system-riscv64",
            words(&[
                "-M",
                "virt",
                "-m",
                "256M",
                "-bios",
                "none",
                "-nographic",
                "-global",
                "virtio-mmio.force-legacy=false",
                "-kernel",
            ]),
        ),
    };
    let device = |mac: Option<&str>| {
        let mut device = String::from("virtio-net-device,netdev=net0");
        if board == Board::ThreadxRiscv64 {
            device.push_str(",bus=virtio-mmio-bus.0");
        }
        if let Some(mac) = mac {
            device.push_str(",mac=");
            device.push_str(mac);
        }
        device
    };
    let mut before = before;
    let after = match net {
        Net::None => words(&["-nic", "none"]),
        Net::User { mac } => vec![
            "-netdev".to_string(),
            "user,id=net0".to_string(),
            "-device".to_string(),
            device(mac),
        ],
        Net::Dgram { local, remote, mac } => {
            // `-icount shift=auto` keeps a timer aligned with SLIRP's host I/O
            // during WFI. A dgram link has no SLIRP, and under icount a burst of
            // RTPS traffic advances virtual time by milliseconds per wall
            // second, so a 1 Hz publish timer fired about once per 30 s
            // (phase-160.K, measured on NuttX virt). Wall-clock it is.
            if let Some(i) = before.iter().position(|a| a == "-icount") {
                before.drain(i..i + 2);
            }
            vec![
                "-netdev".to_string(),
                format!(
                    "dgram,id=net0,local.type=unix,local.path={local},\
                     remote.type=unix,remote.path={remote}"
                ),
                "-device".to_string(),
                device(Some(mac)),
            ]
        }
    };
    Line {
        exe,
        before_kernel: before,
        after_kernel: after,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("nros-qemu-ladder-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    #[test]
    fn the_ladder_is_env_then_current_prefix_then_store_and_never_path() {
        let d = tmp("ladder");
        let store = d.join("store/qemu/11.0");
        touch(&store.join("bin/qemu-system-arm"));
        let none = |_: &str| None;
        let ctx = Ctx {
            getenv: &none,
            project_prefix: Some(d.join("build/qemu")),
            pin: Some("11.0".into()),
            store_dir: Some(store.clone()),
            store_targets: None,
            stale_prefix: StalePrefix::Skip,
        };
        // No prefix: the store answers.
        assert_eq!(
            locate("qemu-system-arm", &ctx),
            QemuBin::Found {
                path: store.join("bin/qemu-system-arm"),
                rung: "store"
            }
        );
        // A STALE prefix is skipped, not taken…
        touch(&d.join("build/qemu/bin/qemu-system-arm"));
        std::fs::write(d.join("build/qemu/.nros-provenance"), "version = \"9.0\"\n").unwrap();
        assert!(matches!(
            locate("qemu-system-arm", &ctx),
            QemuBin::Found { rung: "store", .. }
        ));
        // …or refused, when the caller says so.
        let refusing = Ctx {
            stale_prefix: StalePrefix::Refuse,
            getenv: &none,
            project_prefix: ctx.project_prefix.clone(),
            pin: ctx.pin.clone(),
            store_dir: ctx.store_dir.clone(),
            store_targets: None,
        };
        match locate("qemu-system-arm", &refusing) {
            QemuBin::RefusedPrefix { why, .. } => assert!(why.contains("holds 9.0"), "{why}"),
            other => panic!("{other:?}"),
        }
        // A current one wins over the store.
        std::fs::write(
            d.join("build/qemu/.nros-provenance"),
            "version = \"11.0\"\n",
        )
        .unwrap();
        assert!(matches!(
            locate("qemu-system-arm", &ctx),
            QemuBin::Found {
                rung: "project",
                ..
            }
        ));
        // The env override beats both.
        let env = |k: &str| (k == "QEMU_SYSTEM_ARM").then(|| OsString::from("/opt/q"));
        let ctx_env = Ctx {
            getenv: &env,
            ..ctx
        };
        assert_eq!(
            locate("qemu-system-arm", &ctx_env),
            QemuBin::Found {
                path: "/opt/q".into(),
                rung: "env"
            }
        );
        // Nothing provisioned for riscv64: missing, naming the remedy — never PATH.
        match locate("qemu-system-riscv64", &ctx_env) {
            QemuBin::Missing { why } => assert!(why.contains("nros setup --tool qemu"), "{why}"),
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A target the store does not build is the host's — never `nros setup
    /// --tool qemu`, which would provision a QEMU without it — unless the env
    /// override names one.
    #[test]
    fn a_target_the_store_does_not_build_is_the_hosts() {
        assert_eq!(
            targets_of_configure(
                "./configure --prefix={prefix} --target-list=arm-softmmu,riscv64-softmmu --x"
            ),
            Some(vec!["arm".to_string(), "riscv64".to_string()])
        );
        let none = |_: &str| None;
        let ctx = Ctx {
            getenv: &none,
            project_prefix: None,
            pin: None,
            store_dir: None,
            store_targets: Some(vec!["arm".into(), "riscv64".into()]),
            stale_prefix: StalePrefix::Skip,
        };
        match locate("qemu-system-riscv32", &ctx) {
            QemuBin::Host { why } => {
                assert!(why.contains("apt install qemu-system-misc"), "{why}");
                assert!(!why.contains("nros setup"), "{why}");
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            locate("qemu-system-arm", &ctx),
            QemuBin::Missing { .. }
        ));
        let env = |k: &str| (k == "QEMU_SYSTEM_RISCV32").then(|| OsString::from("/opt/rv32"));
        let ctx = Ctx {
            getenv: &env,
            ..ctx
        };
        assert!(matches!(
            locate("qemu-system-riscv32", &ctx),
            QemuBin::Found { rung: "env", .. }
        ));
    }

    /// The three lines, spelled out once. Both consumers' tests compare what
    /// they launch or print against [`board_line`]; this pins the line itself,
    /// so a change to it is a reviewed diff here and not a silent move in both.
    #[test]
    fn the_board_lines() {
        let user = Net::User { mac: None };
        assert_eq!(
            board_line(Board::NuttxArmVirt, user).argv("K").join(" "),
            "-M virt -cpu cortex-a7 -nographic -icount shift=auto -kernel K \
             -netdev user,id=net0 -device virtio-net-device,netdev=net0"
        );
        assert_eq!(board_line(Board::NuttxArmVirt, user).exe, "qemu-system-arm");
        assert_eq!(
            board_line(Board::NuttxRvVirt, Net::None)
                .argv("K")
                .join(" "),
            "-M virt -bios none -nographic -icount shift=auto -kernel K -nic none"
        );
        assert_eq!(
            board_line(Board::NuttxRvVirt, user).exe,
            "qemu-system-riscv32"
        );
        let mac = threadx_riscv64_mac(1);
        assert_eq!(mac, "52:54:00:12:34:57");
        assert_eq!(
            board_line(Board::ThreadxRiscv64, Net::User { mac: Some(&mac) })
                .argv("K")
                .join(" "),
            "-M virt -m 256M -bios none -nographic -global virtio-mmio.force-legacy=false \
             -kernel K -netdev user,id=net0 \
             -device virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0,mac=52:54:00:12:34:57"
        );
        assert_eq!(
            board_line(Board::ThreadxRiscv64, user).exe,
            "qemu-system-riscv64"
        );
    }

    /// Issue 2170 — the two-guest dgram link: the board's own machine line,
    /// the AF_UNIX pair after the kernel, and no `-icount` (no SLIRP to align
    /// with). The ThreadX pair's line is byte-for-byte what its launcher
    /// spelled by hand before.
    #[test]
    fn a_dgram_pair_shares_the_boards_machine_line() {
        let dgram = Net::Dgram {
            local: "/t/a.sock",
            remote: "/t/b.sock",
            mac: "52:54:00:12:34:56",
        };
        assert_eq!(
            board_line(Board::ThreadxRiscv64, dgram).argv("K").join(" "),
            "-M virt -m 256M -bios none -nographic -global virtio-mmio.force-legacy=false \
             -kernel K -netdev dgram,id=net0,local.type=unix,local.path=/t/a.sock,\
             remote.type=unix,remote.path=/t/b.sock \
             -device virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0,mac=52:54:00:12:34:56"
        );
        let nuttx = board_line(Board::NuttxArmVirt, dgram).argv("K");
        assert!(!nuttx.iter().any(|a| a == "-icount"), "{nuttx:?}");
        assert!(
            nuttx
                .join(" ")
                .starts_with("-M virt -cpu cortex-a7 -nographic -kernel K")
        );
    }
}
