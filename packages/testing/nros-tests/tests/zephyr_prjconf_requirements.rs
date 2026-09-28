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
        // phase-391 W3 — HEAP_MEM_POOL_SIZE is deliberately NOT required any
        // more: zenoh-pico's z_malloc funnels into the rlsf arena in
        // nros-platform's zephyr_heap (NROS_ZEPHYR_HEAP_SIZE), not k_malloc,
        // and the converted confs set the kernel pool to 0 so it shrinks to
        // Zephyr's own ADD_SIZE floor. Requiring pool > 0 here would force
        // every image to carry BOTH heaps — the exact state W3 removed.
        reqs: &[
            Req::Yes("POSIX_API"),
            Req::Min("MAX_PTHREAD_MUTEX_COUNT", 8),
            Req::Min("MAX_PTHREAD_COND_COUNT", 6),
        ],
        why: "zenoh-pico needs POSIX threads with >=8 pthread mutexes / >=6 \
               condvars (the default 5 mutexes fails with -80 at \
               Executor::open); its heap is the phase-391 rlsf arena, not \
               k_malloc, so the kernel pool may be 0",
    },
    Backend {
        rmw: "cyclonedds",
        reqs: &[Req::Positive("HEAP_MEM_POOL_SIZE"), Req::Yes("POSIX_API")],
        why: "Cyclone DDS uses POSIX threads + sockets and a heap",
    },
    Backend {
        rmw: "xrce",
        // phase-391 W3 — same as zenoh: the allocation funnel is the rlsf
        // arena, so the kernel pool is no longer this backend's heap.
        reqs: &[],
        why: "Micro-XRCE's heap is the phase-391 rlsf arena (funnelled through \
               nros_platform_alloc); transport is plain UDP (no pthread), so \
               nothing here needs the kernel pool",
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
    let root = nros_tests::project_root();
    let mut overlays = Vec::new();
    // phase-470 W5.a (issue 1288) — the workspace overlays MOVED; they did not
    // go away. `examples/workspaces/rust`'s application is generated now, so its
    // `prj-<rmw>.conf` set lives in the bringup's board dir (RFC-0065 D4).
    // Following the move keeps this gate's reach exactly what it was: a path
    // left naming the old directory would collect nothing there, the overall
    // `!overlays.is_empty()` assert would still pass on `examples/zephyr` alone,
    // and the coverage would be gone with no red — issue 0196's shape.
    //
    // phase-470 W5.b1 — `examples/workspaces/features` joins the list, and that
    // is a WIDENING, not a follow-the-move: its three `zephyr_rust_*_entry`
    // packages carried a `prj-zenoh.conf` this gate never read, so the rule
    // ("every Zephyr `prj-<rmw>.conf` meets its backend's requirements") always
    // reached further than the list did. Migrating them put that overlay in the
    // very shape the line above already collects, so there is no reason left to
    // leave it out.
    //
    // phase-470 W5.b2 — `realtime-rust` and `safety` join on the same footing,
    // and for the same reason: their `prj-zenoh.conf` was never in this gate's
    // reach although the rule always covered it, and migrating the image put
    // the file where the line above already looks. All four Rust workspaces
    // with a Zephyr image are now collected, so a fifth is the only way this
    // list can go stale again.
    for base in [
        "examples/zephyr",
        "examples/workspaces/rust/src/demo_bringup/boards",
        "examples/workspaces/features/src/demo_bringup/boards",
        "examples/workspaces/realtime-rust/src/demo_bringup/boards",
        "examples/workspaces/safety/src/demo_bringup/boards",
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
