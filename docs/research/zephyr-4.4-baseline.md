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
