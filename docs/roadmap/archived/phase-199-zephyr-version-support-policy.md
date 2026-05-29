# Phase 199 — Zephyr version-support policy (systematic new-version support)

**Goal.** Replace the ad-hoc, per-version-forked Zephyr build path with a
**systematic version-support policy**: define the one contract that bounds which
Zephyr versions nano-ros supports, build only on stable surfaces, and isolate
(then upstream) the unavoidable churn so adding a new Zephyr version is a bounded
checklist — not an open-ended patch-archaeology project.

**Status.** Archived (2026-05-29) — all host-doable work landed. **199.1, 199.2,
199.3, 199.5 DONE** (the policy doc, the pinned `(zephyr × zephyr-lang-rust)`
pairs, the version-dispatched patch sets, the add-a-line checklist). The two
remaining items are **off-box and deferred** (not doable from this repo now):
**199.4** needs upstream Zephyr PRs, **199.6** needs a change in the ASI repo.
Re-open / file follow-ups when those become actionable. Triggered by an
autoware-safety-island (ASI) integration attempt on its pinned Zephyr 3.6.0,
which can't build the nano-ros Rust integration. Findings verified against the
actual repos (`external/zephyr-lang-rust`, `external/asi`) + Zephyr's release docs.

**Priority.** P2 — unblocks downstream integrators (ASI) and stops the
per-version patch churn from compounding each Zephyr release. No MVP capability
depends on it, but every future Zephyr bump pays the tax until it's done.

**Depends on.** Phase 180.A (the `NROS_ZEPHYR_VERSION` + `west-<ver>.yml`
manifest selector — already the right architecture; this phase formalizes the
policy on top of it).

## Overview

**The decisive constraint (verified).** nano-ros's Zephyr support is not bounded
by our patches — it is bounded by **`zephyr-lang-rust`**, the official in-tree
Rust module our integration links through (`rust_cargo_application()` + the
`zephyr` crate; the generated Zephyr package is a Rust staticlib). That module's
first commit is **2024-09-11 ("Initial support for Rust on Zephyr")**, immediately
after the **Zephyr 3.7.0 LTS** (July 2024). It did not exist for 3.6 (Feb 2024)
or 3.5 (Oct 2023). **There is no nano-ros-on-Zephyr below 3.7** — no amount of
NSOS / native_sim patching changes that.

This is why the ASI attempt fails: ASI's `actuation_module/west.yml` pins its own
zephyr at 3.6.0 (`339cd5a…`) with `import: false` *specifically to override*
nano-ros's 3.7.0 pin — but 3.6.0 has no `modules/lang/rust`, so the nros Rust
staticlib has nothing to link into.

**Zephyr's release model** ([releases](https://docs.zephyrproject.org/latest/releases/index.html),
[release process](https://docs.zephyrproject.org/latest/project/release_process.html)):
6-month majors (Apr/Oct); **LTS every ~2.5–3 yr, ~5 yr support** (3.7 is the
current LTS). **Stable APIs** (versioned ≥1.0.0) are frozen ≥2 releases and only
extended; **native_sim / NSOS / driver source are explicitly not stable** — which
is exactly the source our patches edit, hence the per-version re-anchoring.

## Architecture — the contract + stable/churn split

**The version contract = the `(zephyr × zephyr-lang-rust)` compatibility pair.**
A supported configuration is a zephyr revision *plus* a zephyr-lang-rust revision
known to build against it. Our `west-<ver>.yml` already pins both; the policy
makes the *pair* the unit of support.

**Stable — build only on these (version-invariant):**
- Zephyr **stable public APIs** (≥1.0.0): kernel/`k_*`, Kconfig, devicetree, the
  public POSIX + BSD-socket API.
- The **`zephyr-lang-rust` contract**: `rust_cargo_application()`, the `zephyr`
  crate (`0.1.x`), the staticlib build shape. This is the single integration
  boundary; its supported-Zephyr range *is* our support window.
- The **manifest selector** (`NROS_ZEPHYR_VERSION` → `west-<ver>.yml` + workspace
  dir) and the `cmake/platform/` + Kconfig/DT overlay glue.

**Churn — isolate + upstream (the only real per-version cost):**
- native_sim / NSOS internal patches (`drivers/net/nsos_sockets.c`,
  `native_simulator/.../nsos_adapt.c`: recvmsg, getsockname, getifaddrs, mcjoin,
  IPPROTO_IP) — the CycloneDDS-on-**native_sim** host-networking shim. **Simulator
  only** — real targets (FVP `fvp_baser_aemv8r_smp`, NXP S32Z) don't use them.
- The `pthread_mutex_unlock` POSIX-semantics patch + SoC files (Cortex-A9 Zynq).

## Work items

- [x] **199.1 — Document the support policy. DONE** (2026-05-29). Wrote
      `docs/development/zephyr-version-support.md`: the `(zephyr × zephyr-lang-rust)`
      contract; the floor (3.7 LTS — where the Rust module was born, 2024-09-11);
      the window = **current LTS (default/CI) + ≤1 rolling** within
      zephyr-lang-rust's range; stable-surfaces-vs-churn; the per-line dispatch;
      and the add-a-line checklist (199.5, folded in). Cross-linked from `CLAUDE.md`
      (Build) + the `just/zephyr.just` version-selector header.
- [x] **199.2 — Pin the pair explicitly. DONE** (2026-05-29). `west.yml` (3.7
      LTS) now pins `zephyr-lang-rust` to the SHA `main` resolved to
      (`404fcefd…`) instead of the drifting `revision: main`;
      `west-4.4.yml` was already pinned (`a763400f…`). zephyr-lang-rust is
      untagged (no Zephyr-version release to pin), so a SHA is the only handle —
      each `west-<line>.yml` is now a reproducible `(zephyr@<rev>,
      zephyr-lang-rust@<rev>)` pair; bump deliberately.
- [x] **199.3 — Version-dispatched patch sets. DONE** (2026-05-29). The inline
      `if NROS_ZEPHYR_VERSION = 4.4 … else …` patch branch in `just/zephyr.just`
      is replaced by a single dispatcher: `bash scripts/zephyr/patches/${NROS_ZEPHYR_VERSION}.sh
      "$WORKSPACE"` (with an explicit "no patch set for <version>" error listing
      supported lines). The two per-line sequences were lifted verbatim into
      `scripts/zephyr/patches/3.7.sh` + `4.4.sh` (each `cd`s to repo root, takes
      the workspace arg, keeps every patch idempotent). Adding a Zephyr line =
      drop a sibling `patches/<version>.sh` — **no edit to the recipe**; contract
      in `scripts/zephyr/patches/README.md`. The individual `scripts/zephyr/*.sh`
      patch scripts stay in place (the per-line sets call them — moving them risks
      their cwd/relative-path assumptions, and the dispatch goal is met without
      it). Verified `bash -n` clean on both sets, `just --show zephyr::setup`
      parses, the missing-version + usage guards fire. *Remaining (minor):*
      `scripts/zephyr/setup.sh` still has one `if MANIFEST = west.yml` gate for the
      Cortex-A9 patch — fold into the 3.7 set in a follow-up (it's idempotently
      re-applied by `patches/3.7.sh` anyway).
- [~] **199.4 (deferred — off-box: upstream Zephyr PRs) — Upstream the native_sim / NSOS fixes to Zephyr.** They are
      genuine native_sim bug fixes (UDP recvmsg, IPv4-multicast SPDP, getsockname/
      getifaddrs, mcjoin mreq). File upstream PRs; track which land per release so
      the carried set *shrinks* each version instead of forking. Mark each patch in
      `patches/<version>/` with its upstream PR / merged-in-version.
- [x] **199.5 — "Add a Zephyr version" checklist. DONE** (2026-05-29). The
      bounded checklist lives in `docs/development/zephyr-version-support.md`
      (§"Adding a new Zephyr line") + `scripts/zephyr/patches/README.md`: confirm
      the `(zephyr × zephyr-lang-rust)` pair builds (stop if < 3.7); pin the pair
      in `west-<line>.yml`; wire the `NROS_ZEPHYR_VERSION` selector arms; drop
      `scripts/zephyr/patches/<line>.sh` (no recipe edit); add a CI line; sources
      via `nros setup --source`.
- [~] **199.6 (deferred — off-box: ASI repo change) — ASI alignment (downstream, coordinate).** ASI's
      `actuation_module/west.yml` must drop the `import: false` override (or bump
      its zephyr pin to **v3.7.0**) so its C++ nano-ros build links the nros Rust
      staticlib. Lands in the ASI repo (`NEWSLabNTU/autoware-safety-island`
      `nano-ros` branch), not here — document the required manifest change + the
      reason (no `modules/lang/rust` below 3.7). Combine with 197.1 (local
      `just zephyr setup` provisions sources) so a fresh ASI clone builds.

## Acceptance

- [ ] `docs/development/zephyr-version-support.md` defines the
      `(zephyr × zephyr-lang-rust)` contract, the 3.7 floor, the LTS+rolling
      window, and the add-a-version checklist; CLAUDE.md + `just/zephyr.just` link it.
- [x] Every `west-<ver>.yml` pins a specific (not `main`) zephyr-lang-rust
      revision — reproducible pairs (199.2).
- [ ] Version-specific patches live under `scripts/zephyr/patches/<version>/`
      behind one applier; `just zephyr setup` works for each supported line off a
      fresh clone (with 197.1).
- [ ] At least the native_sim NSOS fixes have upstream PRs filed + tracked.
- [ ] ASI builds nano-ros on the `(3.7 LTS × zephyr-lang-rust)` pair (downstream
      verification; gated on 199.6 landing in the ASI repo).

## Notes

- **Why not support 3.5/3.6:** the official Rust module didn't exist yet. Anyone
  needing nano-ros on Zephyr is on ≥3.7 by construction. This isn't a nano-ros
  limitation to "fix" — it's the upstream reality of when Rust support landed.
- **native_sim patches are dev/test-only.** Real safety-island targets (FVP
  aemv8r, S32Z R52) use the hardware net stack, not NSOS — so the churniest
  patches don't gate production hardware, only the host simulator/CI path.
- The compatibility pair, not the zephyr version alone, is the support unit:
  bumping zephyr without a matching zephyr-lang-rust (or vice-versa) is unsupported.
