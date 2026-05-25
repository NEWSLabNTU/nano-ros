# Zephyr 4.4 baseline build (Phase 180.A Task 3)

**Purpose.** Empirically build the simplest example (`examples/zephyr/c/talker`,
zenoh, `native_sim/native/64`, host toolchain) on the freshly-stood-up
Zephyr 4.4 workspace to surface the real failure set that drives Tasks 4–9.
Companion to the static `zephyr-3.7-to-4.4-divergence-audit.md`.

**Date.** 2026-05-25. **Branch.** `phase-180a-version-spanning-module`.
**Workspace.** `../nano-ros-workspace-4.4` (Zephyr 4.4.0).

## How far the build gets

After clearing the env prerequisites below, configure reaches the Kconfig
stage and stops at the first symbol divergence:

```
-- Found Python3: .../.venv312/bin/python (3.12.12)
-- Zephyr version: 4.4.0
-- Found toolchain: host (gcc/ld)
-- Found BOARD.dts: .../native_sim/native_sim_64.dts   (devicetree OK)
prj-zenoh.conf:7: warning: attempt to assign the value '16' to the undefined symbol MAX_PTHREAD_COUNT
error: Aborting due to Kconfig warnings
```

So on 4.4 the module gets through toolchain selection, devicetree, and into
Kconfig before the first real break. Toolchain (`host` = host gcc) and
devicetree are fine.

## Environment prerequisites (4.4 line)

These are hard prereqs — not code issues — and gate every 4.4 build:

1. **Python ≥ 3.12.** Zephyr 4.4's `find_package(Python3)` requires 3.12
   (host has 3.10.12 → configure aborts before Kconfig). Provisioned **without
   sudo** via the uv-managed 3.12 already on this box:
   ```bash
   uv venv --python 3.12 <ws>/.venv312
   uv pip install --python <ws>/.venv312/bin/python west pyelftools -r zephyr/scripts/requirements.txt
   <ws>/.venv312/bin/python -m west build ...      # run west THROUGH the venv
   ```
   Gotchas hit: (a) `uv venv` creates **no pip** — use `uv pip install` to
   target the venv, not `pip`; (b) bare `west` resolves to the 3.10
   `~/.local/bin/west` and hands cmake `WEST_PYTHON=/usr/bin/python3` — must
   invoke `python -m west` through the venv interpreter so `WEST_PYTHON` is
   3.12.
2. **dtc** (devicetree compiler) not found — emitted as a warning only;
   native_sim proceeded. Install for cleaner builds, non-blocking.

**Feedback to the plan:** Task 2's `just zephyr setup NROS_ZEPHYR_VERSION=4.4`
is incomplete — it does not provision Python 3.12, so it would fail here.
Add a Python-3.12 provisioning step (uv venv) to the 4.4 setup path. New
work item: **180.A Task 2b — Python 3.12 provisioning for the 4.4 line.**

## First Kconfig divergence (resolves audit Task 8)

The audit flagged the POSIX/pthread Kconfig as High-risk "needs a live tree."
Resolved here against 4.4 `lib/posix`:

| Overlay symbol | 4.4 status |
| --- | --- |
| `CONFIG_POSIX_API` | present (`lib/posix/Kconfig.profile`) |
| `CONFIG_MAX_PTHREAD_MUTEX_COUNT` | present |
| `CONFIG_MAX_PTHREAD_COND_COUNT` | present |
| `CONFIG_POSIX_THREAD_THREADS_MAX` | present |
| **`CONFIG_MAX_PTHREAD_COUNT`** | **REMOVED** |

Only one symbol is gone. 4.4 has `MAX_PTHREAD_{BARRIER,COND,MUTEX,RWLOCK,
SPINLOCK}_COUNT` but no bare `MAX_PTHREAD_COUNT`; the live thread-count cap is
now `CONFIG_POSIX_THREAD_THREADS_MAX` (already set to 16 in our overlays). So
**the fix is to drop `CONFIG_MAX_PTHREAD_COUNT` on the 4.4 line** — the count
is already covered. This is a one-symbol removal, not the broad rename the
audit feared; **Task 8 collapses to nearly nothing.**

Because `prj-*.conf` overlays are shared by both Zephyr lines and
`MAX_PTHREAD_COUNT` still exists on 3.7, the removal must be **version-aware**
(a 4.4 overlay/snippet, not an edit to the shared file) — that mechanism is
Task 8 / Phase 180.C, so the fix is deferred to there rather than applied in
this baseline.

## Failure set status

- **Resolved/characterized:** env prereqs (Python 3.12, dtc); first Kconfig
  break (`MAX_PTHREAD_COUNT` removed).
- **Not yet observable:** deeper Kconfig / header / API / NSOS failures are
  gated behind the `MAX_PTHREAD_COUNT` fix; they surface on the next rebuild
  once a version-aware 4.4 overlay drops that symbol (Task 8). The baseline is
  iterative by nature — each fix reveals the next layer.

## Conclusion

The module reaches Kconfig on 4.4 after env provisioning; toolchain +
devicetree are clean. The audit's two High-risk unknowns are now bounded:
POSIX Kconfig = a single removed symbol (this doc); Rust module = still
pending its own live-tree check (Task 9). The 4.4 line needs a Python-3.12
provisioning step folded into setup (Task 2b).

## Update — Task 8 applied (2026-05-25)

`CONFIG_MAX_PTHREAD_COUNT` removed/migrated across 42 non-vendored confs
(deleted where `POSIX_THREAD_THREADS_MAX` was already set — redundant since
3.7's deprecated `MAX_PTHREAD_COUNT` defaults to it; migrated to
`POSIX_THREAD_THREADS_MAX` in the one conf lacking it). Behavior-preserving
on 3.7, valid on 4.4. Vendored zenoh-pico doc conf left untouched.

Rebuild result: **Kconfig now completes on 4.4** (autoconf.h generated), and
the build advances through host gcc/ld/asm detection to the **next blocker —
`nros-codegen not found`** (`zephyr/cmake/nros_generate_interfaces.cmake:95`).
That is a generic host-tool prerequisite common to *both* Zephyr lines (the
`build-fixtures` recipe builds the host codegen tool and passes
`_NANO_ROS_CODEGEN_TOOL`), **not** a 4.4 divergence — so it belongs to the
build-orchestration/version-gating work (Tasks 4–10), not the POSIX fix.

No further 4.4-specific Kconfig divergence observed up to the codegen gate.

## Update — orchestration: first green 4.4 build (2026-05-25)

With the host `nros-codegen` built and passed via
`-D_NANO_ROS_CODEGEN_TOOL`, **c/talker zenoh builds end-to-end on Zephyr
4.4** (`native_sim/native/64`, host toolchain): 1303 ninja steps through
nros cargo (Corrosion) + zenoh-pico + std_msgs C codegen → `zephyr.elf`
(27 MB) + `zephyr.exe`. It also **boots and runs**: `*** Booting Zephyr OS
build v4.4.0 ***`, nros C talker initializes and reaches the network-wait
(fails to connect only because no zenohd/NSOS overlay in the smoke run —
expected). 3 non-fatal warnings. The zenoh native_sim line needed **zero
NSOS patches** (TCP transport).

**Tasks 4 + 7 resolved by this build:**
- **Task 4** (drop the upstreamed getsockname patch): confirmed — `getsockname`
  is upstream in 4.4 and zenoh built/booted without the patch.
- **Task 7** (socket/native_sim Kconfig + header renames): **no-op** — none of
  `CONFIG_NET_SOCKETS_POLL_MAX`, `zephyr/net/buf.h`,
  `CONFIG_NATIVE_SIM_NATIVE_POSIX_COMPAT`, `CONFIG_NATIVE_APPLICATION` are used
  anywhere in nano-ros examples / module / in-tree (0 files); the clean build
  corroborates. Nothing to rename.

**Tasks 5–6 (recvmsg reshape, IP-multicast + getifaddrs re-anchor): deferred,
runtime-only.** They do not block compile/link (verified — zenoh builds, and
the symbols are runtime mid-handling, not link deps). They matter only for the
**cyclonedds** native_sim path (RTPS multicast discovery via NSOS). Verifying
them requires the full cyclonedds-on-4.4 bring-up (cyclonedds submodule patches,
host idlc, cmake glue) plus a 2-node runtime discovery test — a separate effort
tracked alongside the cyclonedds-4.4 work, not the zenoh baseline.

## Update — orchestration polish: `just zephyr build-one` (2026-05-25)

Added a focused `build-one <example> <rmw>` recipe (`just/zephyr.just`) that
reproduces the orchestration first-class on either line: it resolves the
versioned workspace, (4.4) prepends the Python 3.12 venv to PATH + selects the
host toolchain, builds host `nros-codegen`, and `west build`s the example with
`-D_NANO_ROS_CODEGEN_TOOL`. **Verified on 4.4:**
`NROS_ZEPHYR_VERSION=4.4 just zephyr build-one c/talker zenoh` →
`zephyr.elf`. The 3.7 path uses the established ambient toolchain (same,
simpler branch — not re-run here). The whole `build-fixtures` matrix is *not*
version-gated yet (it inlines 3.7 patches + builds C++/cyclonedds that aren't
4.4-ready); `build-one` is the reproducible 4.4 entry point meanwhile.

### First version-aware-overlay case: ETH_NATIVE_POSIX → ETH_NATIVE_TAP

Adding the NSOS board overlay (`boards/native_sim_native_64.conf`) to the 4.4
build surfaced a genuine version-divergent symbol: the overlay sets
`CONFIG_ETH_NATIVE_POSIX=n` (disable the native eth driver so NSOS offload is
used), but 4.4 renamed it `CONFIG_ETH_NATIVE_TAP` (driver `eth_native_posix`
→ `eth_native_tap`). Unlike `MAX_PTHREAD_COUNT` (deprecated/redundant →
deletable), this `=n` is **meaningful on both lines**, so it cannot just be
dropped — it needs a per-line value in a SHARED overlay. **This is the first
concrete case that requires the version-aware overlay mechanism** Task 8
deferred (a 4.x snippet / per-line conf selection — Phase 180.C). **RESOLVED (version-aware overlay mechanism, 2026-05-25).** Added per-line
native_sim overlays `cmake/zephyr/native-sim-line-{3.7,4.4}.conf` (identical
NSOS settings; only the eth-disable symbol differs —
`CONFIG_ETH_NATIVE_POSIX=n` vs `CONFIG_ETH_NATIVE_TAP=n`). `build-one` selects
the file by `NROS_ZEPHYR_VERSION` and appends it to `CONF_FILE` for native_sim
boards, superseding the legacy per-example `boards/native_sim_*.conf`.
**Verified:** `NROS_ZEPHYR_VERSION=4.4 just zephyr build-one c/talker zenoh`
builds clean and boots with **`Network ready (NSOS — host kernel sockets)`**
— NSOS is now active on 4.4 (previously the tap driver failed on
`eth_tap: Cannot create zeth`). This is the reusable hook for any future
per-line config divergence (extend the two line confs); the broader
snippet-based form remains Phase 180.C. The legacy per-example board overlays
are untouched, awaiting the not-yet-version-gated `build-fixtures` path.
