# Phase 180.A — Version-Spanning Zephyr Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the nano-ros Zephyr module build and `just zephyr test` green on **both** Zephyr 3.7 LTS and Zephyr 4.4, version-gating only where the two diverge.

**Architecture:** Stand up a 4.4 workspace alongside the existing 3.7 one; parametrize the `just zephyr` recipes by a `NROS_ZEPHYR_VERSION` selector; make each of the 16 patch scripts version-aware (drop obsolete, reshape, or re-anchor per the divergence audit); rename the handful of Kconfig/header symbols that moved.

**Tech Stack:** Zephyr (west, CMake, Kconfig), `native_sim` target, `zephyr-lang-rust`, bash patch scripts, nextest.

**Grounding:** `docs/research/zephyr-3.7-to-4.4-divergence-audit.md` (Phase 180.A.0). Read it first.

---

## File Structure

- `just/zephyr.just` — add `NROS_ZEPHYR_VERSION` selector + per-version workspace dirs; gate patch application.
- `west.yml` / a sibling `west-4.4.yml` — pin the 4.4 manifest (Zephyr + `zephyr-lang-rust` revs).
- `scripts/zephyr/*-patch.sh` — version-aware (the 16 scripts; NSOS + Rust families change most).
- `examples/zephyr/*/prj*.conf`, `examples/zephyr/*/boards/native_sim*.conf` — Kconfig renames.
- `zephyr/CMakeLists.txt`, `zephyr/Kconfig` — version guards where the module touches moved APIs.

> The two HIGH-risk items (Rust module CMake shape, POSIX/pthread Kconfig renames) cannot be written with exact fixes until a live 4.4 tree exists. Tasks 8–9 are *investigative tasks with concrete commands and a defined deliverable*, followed by a **re-plan checkpoint** — not speculative fixes.

---

### Task 1: Stand up a Zephyr 4.4 workspace alongside 3.7

**Files:** Create `west-4.4.yml`; Modify `just/zephyr.just`.

- [ ] **Step 1: Pin the 4.4 manifest.** Copy `west.yml` → `west-4.4.yml`, set `zephyr` `revision: v4.4.0`, and pin `zephyr-lang-rust` to the rev that the Zephyr 4.4 manifest references (look it up in `zephyrproject-rtos/zephyr@v4.4.0/west.yml` / the lang-rust release matching 4.4 — do NOT leave it at `main`).
- [ ] **Step 2: Init the 4.4 workspace.**

```bash
NROS_ZEPHYR_WORKSPACE=zephyr-workspace-4.4 \
  west init -l --mf west-4.4.yml .
cd zephyr-workspace-4.4 && west update --narrow -o=--depth=1 && west zephyr-export
```

Expected: a populated `zephyr-workspace-4.4/zephyr` at v4.4.0.

- [ ] **Step 3: Verify the tree version.**

Run: `cat zephyr-workspace-4.4/zephyr/VERSION`
Expected: `VERSION_MAJOR = 4` / `VERSION_MINOR = 4`.

- [ ] **Step 4: Commit.**

```bash
git add west-4.4.yml just/zephyr.just
git commit -m "build(zephyr): pin 4.4 workspace manifest alongside 3.7 LTS"
```

### Task 2: Parametrize `just zephyr` by Zephyr version

**Files:** Modify `just/zephyr.just`.

- [ ] **Step 1: Add a version selector.** Add `NROS_ZEPHYR_VERSION` (values `3.7`|`4.4`, default `3.7`) that picks the workspace dir (`zephyr-workspace` vs `zephyr-workspace-4.4`) and the manifest. Thread it through `setup`, `build`, `test`.
- [ ] **Step 2: Gate patch application by version.** In `setup`, branch the patch-script block on `NROS_ZEPHYR_VERSION` (3.7 → existing set; 4.4 → the version-aware set from Tasks 4–9).
- [ ] **Step 3: Verify selection.**

Run: `NROS_ZEPHYR_VERSION=4.4 just zephyr doctor`
Expected: reports the `zephyr-workspace-4.4` path and a v4.4 tree.

- [ ] **Step 4: Commit.**

```bash
git add just/zephyr.just && git commit -m "build(zephyr): NROS_ZEPHYR_VERSION selector for 3.7/4.4"
```

### Task 3: Baseline build one example on 4.4 (drives the rest)

**Files:** none (investigative).

- [ ] **Step 1: Attempt the simplest build.**

Run: `NROS_ZEPHYR_VERSION=4.4 just zephyr build` (or the narrowest `west build -b native_sim examples/zephyr/c/talker -- -DCONF_FILE="prj.conf;prj-zenoh.conf"`)
- [ ] **Step 2: Capture the failure set** into `tmp/zephyr-4.4-baseline.txt`. Categorize each failure: Kconfig-not-found, header-not-found, patch-anchor-miss, API-removed. This list confirms/extends Tasks 4–9.
- [ ] **Step 3: Commit the baseline note** (under `docs/research/` if useful, else keep in `tmp/`).

### Task 4: NSOS — drop the now-upstream getsockname patch

**Files:** Modify `just/zephyr.just` (4.4 patch block); keep `scripts/zephyr/nsos-getsockname-patch.sh` for 3.7.

- [ ] **Step 1:** Exclude `nsos-getsockname-patch.sh` from the 4.4 patch set (audit: `nsos_getsockname` + `nsos_adapt_getsockname` are upstream in v4.4.0).
- [ ] **Step 2: Verify** a 4.4 build no longer needs it and getsockname still resolves (the example that uses NSOS getsockname links).
- [ ] **Step 3: Commit.**

```bash
git commit -am "fix(zephyr-4.4): drop nsos-getsockname patch (upstreamed in 4.4)"
```

### Task 5: NSOS — reshape recvmsg to fill the 4.4 ENOTSUP stub

**Files:** Create `scripts/zephyr/nsos-recvmsg-patch-4.4.sh` (or version-branch the existing).

- [ ] **Step 1:** v4.4.0 has `nsos_recvmsg` as a stub returning `ENOTSUP`. Write a 4.4 variant that *replaces the stub body* with the real implementation (the 3.7 patch ADDS the function; the 4.4 patch fills the existing one). Anchor on the stub signature.
- [ ] **Step 2: Verify** the patch applies idempotently to the 4.4 `nsos_sockets.c` and recvmsg returns data, not ENOTSUP.
- [ ] **Step 3: Commit.**

### Task 6: NSOS — re-anchor IP-multicast (×3) + getifaddrs to 4.4

**Files:** version-branch `native-sim-ipproto-ip-patch.sh`, `nsos-adapt-ipproto-ip-patch.sh`, `nsos-mcjoin-mreq-patch.sh`, `nsos-getifaddrs-patch.sh`.

- [ ] **Step 1:** v4.4.0 `nsos_setsockopt`/`nsos_adapt_setsockopt` switch on SOL_SOCKET/TCP/IPV6 (no IPPROTO_IP, no getifaddrs). Re-anchor each patch's grep/sed target to the 4.4 switch shape (now includes the IPV6 case — insert the IP case beside it). Logic is unchanged; only the anchor strings move.
- [ ] **Step 2: Verify** each applies idempotently and `IP_ADD_MEMBERSHIP` + `getifaddrs` resolve; the cyclonedds native_sim example joins `239.255.0.1`.
- [ ] **Step 3: Commit.**

### Task 7: Socket / native_sim Kconfig + header renames

**Files:** `examples/zephyr/*/prj*.conf`, `boards/native_sim*.conf`, `zephyr/CMakeLists.txt`, module sources.

- [ ] **Step 1:** Apply the migration-guide renames on the 4.4 path: `CONFIG_NET_SOCKETS_POLL_MAX` → `CONFIG_ZVFS_POLL_MAX` (if present); `#include <zephyr/net/buf.h>` → `<zephyr/net_buf.h>`; drop/replace `CONFIG_NATIVE_SIM_NATIVE_POSIX_COMPAT` reliance. Grep first: `grep -rn "NET_SOCKETS_POLL_MAX\|net/buf.h\|NATIVE_POSIX_COMPAT\|NATIVE_APPLICATION" examples/zephyr zephyr packages/core/nros-platform-zephyr`.
- [ ] **Step 2: Verify** the 4.4 build advances past these symbols.
- [ ] **Step 3: Commit.**

### Task 8: POSIX/pthread Kconfig — diff live 4.4, apply renames (investigative → fix)

**Files:** `examples/zephyr/*/prj*.conf`.

- [ ] **Step 1: Diff the symbols.** For each of `CONFIG_POSIX_API`, `CONFIG_MAX_PTHREAD_MUTEX_COUNT`, `CONFIG_MAX_PTHREAD_COND_COUNT`, `CONFIG_MAX_PTHREAD_COUNT`, `CONFIG_POSIX_THREAD_THREADS_MAX`: check existence in the 4.4 tree.

```bash
for s in POSIX_API MAX_PTHREAD_MUTEX_COUNT MAX_PTHREAD_COND_COUNT MAX_PTHREAD_COUNT POSIX_THREAD_THREADS_MAX; do
  echo "== $s =="; grep -rl "config $s\$" zephyr-workspace-4.4/zephyr/lib/posix zephyr-workspace-4.4/zephyr/**/Kconfig* 2>/dev/null || echo MISSING
done
```

- [ ] **Step 2: Record** the old→new mapping for any MISSING symbol (the POSIX subsystem granularized; expect `CONFIG_MAX_PTHREAD_*` renames). Deliverable: a rename table.
- [ ] **Step 3: Apply** the renames to the 4.4 prj overlays (version-branch the overlay or use a snippet — final mechanism decided in Phase 180.C).
- [ ] **Step 4: Verify + commit** the 4.4 build resolves all POSIX/pthread symbols.

### Task 9: Rust module — pin + re-verify the 4 patch anchors (investigative → CHECKPOINT)

**Files:** `west-4.4.yml`, `scripts/zephyr/{aarch64,cortex-a9,cortex-r,cargo-features}-rust-patch.sh`.

- [ ] **Step 1: Locate the live shape.** Open `zephyr-workspace-4.4/modules/lang/rust/CMakeLists.txt` (the official 4.1+ module). For each of the 4 Rust patches, check whether its grep sentinel still matches.

```bash
for p in aarch64 cortex-a9 cortex-r cargo-features; do
  echo "== $p =="; bash scripts/zephyr/$p-rust-patch.sh zephyr-workspace-4.4 2>&1 | tail -3
done
```

- [ ] **Step 2: Record** which anchors still match, which moved, which are obsolete (the official module's CMake differs from the 3.7-era checkout).
- [ ] **Step 3: CHECKPOINT — re-plan.** The exact fixes depend on Step 2's findings; do not guess them here. Produce a short follow-up task list (re-anchor / drop / rewrite per patch) and resume execution. This is the one place 180.A intentionally defers concrete code until the live tree is read.

### Task 10: Dual-line CI

**Files:** `just/zephyr.just` (or the CI workflow), test config.

- [ ] **Step 1:** Add a CI path that runs the Zephyr example matrix on **both** `NROS_ZEPHYR_VERSION=3.7` and `=4.4`.
- [ ] **Step 2: Verify** both legs green (or quarantine known-flaky with an actionable skip, never silent).
- [ ] **Step 3: Commit.**

---

## Self-Review

- **Spec coverage (vs Phase 180 doc 180.A):** audit ✓ (180.A.0, done); version-gate module ✓ (Tasks 2,4–9); parametrize recipes ✓ (Task 2); re-verify 16 patches ✓ (Tasks 4–9 cover NSOS+Rust; cyclone-submodule + llext are low-risk re-test, fold into Task 3 baseline); dual CI ✓ (Task 10).
- **Placeholder scan:** Tasks 8–9 are investigative-with-commands + an explicit re-plan checkpoint, by design (the audit proved their fixes need a live tree). No vague "handle edge cases" steps.
- **Consistency:** `NROS_ZEPHYR_VERSION` selector + `zephyr-workspace-4.4` dir used consistently from Task 1 on.

## Known gate

Task 1 (4.4 workspace bringup) is a heavy, network/disk-bound operation (GB download, long build). It must run before Tasks 3–10 and needs an explicit go-ahead.
