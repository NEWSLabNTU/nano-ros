//! Phase 241.C.2b (Zephyr) — Kconfig "config-agreement" gate for Zephyr examples.
//!
//! The FreeRTOS half (`freertos_capabilities_agree_with_freertosconfig`) cross-
//! checks a board's declared `[board.capabilities]` against its co-located
//! `FreeRTOSConfig.h`. **Zephyr doesn't fit that model**: no Zephyr target has an
//! `nros-board.toml` descriptor (deploy picks a *west board name*), and the
//! capability lives in per-example, per-RMW `prj-<rmw>.conf` Kconfig — there is no
//! board.toml block to diff against. So for Zephyr "config agreement" is
//! reinterpreted as **"each `prj-<rmw>.conf` provides the Kconfig nano-ros's
//! backend actually requires on Zephyr"** — the merge-time analogue of the #38
//! gate, catching the real Zephyr footguns (most notably the zenoh-pico `-80` at
//! `Executor::open` when `CONFIG_MAX_PTHREAD_MUTEX_COUNT` is left at the default 5).
//!
//! Host string-parse only — NO west / Zephyr SDK — so it runs on every PR
//! regardless of the Zephyr CI being red (#58/#59). The effective config for a
//! build is `prj.conf` (base) + `prj-<rmw>.conf` (overlay), so both are merged
//! (overlay wins) before the check.
//!
//! The requirements table (`REQUIREMENTS`) is the single source of truth: add a
//! row to extend coverage; minimums are the documented backend needs, not the
//! values the examples happen to use (so the gate has headroom, not a tautology).

use std::{collections::HashMap, path::PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

/// One Kconfig requirement for a backend's Zephyr `prj-<rmw>.conf`.
enum Req {
    /// `CONFIG_<sym>=y`.
    Yes(&'static str),
    /// `CONFIG_<sym>` set to an integer `>= min`.
    Min(&'static str, i64),
    /// `CONFIG_<sym>` set to an integer `> 0`.
    Positive(&'static str),
}

struct Backend {
    /// `prj-<rmw>.conf` suffix.
    rmw: &'static str,
    reqs: &'static [Req],
    /// Short reason, shown in the failure so the fix is obvious.
    why: &'static str,
}

const REQUIREMENTS: &[Backend] = &[
    Backend {
        rmw: "zenoh",
        // zenoh-pico on the Zephyr POSIX port: pthread mutex/cond per transport
        // (TX/RX/peer) + a write filter per publisher. The default
        // CONFIG_MAX_PTHREAD_MUTEX_COUNT=5 exhausts the pool → `_z_*` returns -80
        // (`_Z_ERR_SYSTEM_GENERIC`) at session open. Needs ~8+; examples use 32/16.
        reqs: &[
            Req::Positive("HEAP_MEM_POOL_SIZE"),
            Req::Yes("POSIX_API"),
            Req::Min("MAX_PTHREAD_MUTEX_COUNT", 8),
            Req::Min("MAX_PTHREAD_COND_COUNT", 6),
        ],
        why: "zenoh-pico needs a heap (k_malloc) + POSIX threads with >=8 pthread \
               mutexes / >=6 condvars; the default 5 mutexes fails with -80 at \
               Executor::open",
    },
    Backend {
        rmw: "cyclonedds",
        reqs: &[Req::Positive("HEAP_MEM_POOL_SIZE"), Req::Yes("POSIX_API")],
        why: "Cyclone DDS uses POSIX threads + sockets and a heap",
    },
    Backend {
        rmw: "xrce",
        reqs: &[Req::Positive("HEAP_MEM_POOL_SIZE")],
        why: "Micro-XRCE needs a heap (k_malloc); transport is plain UDP (no pthread)",
    },
];

/// Parse `CONFIG_<X>=<v>` lines into a map (last wins, mirroring Kconfig merge).
fn parse_kconfig(path: &std::path::Path, into: &mut HashMap<String, String>) {
    let Ok(src) = std::fs::read_to_string(path) else {
        return;
    };
    for line in src.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            if let Some(sym) = k.strip_prefix("CONFIG_") {
                into.insert(sym.to_string(), v.trim().trim_matches('"').to_string());
            }
        }
    }
}

fn as_int(v: &str) -> Option<i64> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        i64::from_str_radix(hex, 16).ok()
    } else {
        v.parse().ok()
    }
}

/// Recursively collect `prj-<rmw>.conf` overlays under `dir`.
fn collect_overlays(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_overlays(&p, out);
        } else if let Some(name) = p.file_name().and_then(|n| n.to_str())
            && name.starts_with("prj-")
            && name.ends_with(".conf")
        {
            out.push(p);
        }
    }
}

#[test]
fn zephyr_prjconf_meets_backend_requirements() {
    let root = repo_root();
    let mut overlays = Vec::new();
    for base in [
        "examples/zephyr",
        "examples/workspaces/rust/src/zephyr_entry",
    ] {
        collect_overlays(&root.join(base), &mut overlays);
    }
    overlays.sort();
    assert!(
        !overlays.is_empty(),
        "no Zephyr `prj-<rmw>.conf` overlays found — the C.2b Zephyr guard is vacuous"
    );

    let mut failures = Vec::new();
    let mut checked = 0usize;
    for overlay in &overlays {
        let rmw = overlay
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix("prj-"))
            .and_then(|n| n.strip_suffix(".conf"))
            .unwrap();
        let Some(backend) = REQUIREMENTS.iter().find(|b| b.rmw == rmw) else {
            continue; // an RMW we don't (yet) have requirements for — skip, not fail
        };

        // Effective config = base prj.conf + the per-RMW overlay (overlay wins).
        let mut cfg = HashMap::new();
        parse_kconfig(&overlay.with_file_name("prj.conf"), &mut cfg);
        parse_kconfig(overlay, &mut cfg);

        let rel = overlay.strip_prefix(&root).unwrap_or(overlay).display();
        for req in backend.reqs {
            let ok = match req {
                Req::Yes(sym) => cfg.get(*sym).map(|v| v == "y").unwrap_or(false),
                Req::Positive(sym) => cfg.get(*sym).and_then(|v| as_int(v)).unwrap_or(0) > 0,
                Req::Min(sym, min) => cfg.get(*sym).and_then(|v| as_int(v)).unwrap_or(-1) >= *min,
            };
            if !ok {
                let (sym, want, got) = match req {
                    Req::Yes(s) => (*s, "=y".to_string(), cfg.get(*s).cloned()),
                    Req::Positive(s) => (*s, "> 0".to_string(), cfg.get(*s).cloned()),
                    Req::Min(s, m) => (*s, format!(">= {m}"), cfg.get(*s).cloned()),
                };
                failures.push(format!(
                    "  {rel} (rmw={rmw}): CONFIG_{sym} must be {want}, got {got:?}\n      → {}",
                    backend.why
                ));
            }
        }
        checked += 1;
    }

    assert!(checked > 0, "no recognised-RMW overlay was checked");
    assert!(
        failures.is_empty(),
        "Zephyr prj-<rmw>.conf requirements not met (the merge-time analogue of the \
         #38 capability gate — catches e.g. the zenoh-pico -80 mutex-count footgun):\n{}",
        failures.join("\n")
    );
}
