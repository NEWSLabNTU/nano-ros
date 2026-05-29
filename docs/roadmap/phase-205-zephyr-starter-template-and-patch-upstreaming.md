# Phase 205 — Zephyr starter-template repo + patch upstreaming

**Goal.** Give Zephyr end-users a zero-friction "clone → build → run" start that
respects Zephyr's own workspace model, and shrink the nano-ros patch surface they
have to carry. Two complementary tracks: (A) a **manifest/app starter-template
repo** (the `example-application` pattern — *not* a vendored, pre-patched Zephyr
fork), and (B) **upstreaming** the generic patches so fewer apply at all.

**Status.** Proposed (2026-05-29). From a design review of the BYO Zephyr
adoption path (Phase 202, e2e-verified): end-users keep their own `west`
workspace; the patches apply via `west patch` / scripts to a *tested* Zephyr
version; a pre-patched fork would be a maintenance trap.

**Priority.** P3 — adoption ergonomics, not a capability gap. 205.A (template) is
the higher-value half; 205.B (upstreaming) is a slow, upstream-paced cleanup.

**Depends on.** Phase 202 (BYO UX — provisioning + patch docs, e2e-verified),
Phase 180.C/.D (snippets + `west patch`), Phase 195/197 (`nros setup`).

## Overview

Two questions drove this:

1. *nano-ros pulls a Zephyr workspace in setup; end-users do the same with their
   own `west` workspace — do the patches still apply?* **Yes, at a tested Zephyr
   version.** The patches edit the *user's* workspace tree
   (`zephyr/drivers/net/nsos_sockets.c`, `modules/lang/rust/…`, the cyclonedds
   fork), delivered by `west patch apply` (4.x, sha256-verified `.patch`) or the
   anchor-based, version-tolerant sed/python scripts (3.7 + rust/cyclonedds).
   Verified in the Phase 202 e2e: the NSOS 3.7 patches applied cleanly to a fresh
   `v3.7.0` clone and `c/talker` ran to `Published: 1`. **The risk is Zephyr-
   version drift** — the anchors/sha are keyed to the tested pins (3.7.0 LTS /
   4.4.0); a user on a different Zephyr commit gets a sha-mismatch (`.patch`) or a
   warn+skip (scripts).

2. *Should we ship a pre-patched Zephyr workspace on GitHub?* **No.** It would
   vendor a modified Zephyr (~GBs) to host + rebase on every Zephyr release (a
   permanent fork), lock the Zephyr version (killing "build against whatever
   Zephyr you pin"), and duplicate `west patch` (which exists precisely to deliver
   downstream patches into the user's own workspace). The right artifact is a
   *manifest + app skeleton* starter, no vendored Zephyr.

## Work Items

### 205.A — [P3] Zephyr starter-template repo (`example-application` pattern)
A small public repo (e.g. `NEWSLabNTU/nano-ros-zephyr-example`) that bootstraps a
BYO workspace without vendoring Zephyr:
- [ ] `west.yml` pinning a **tested Zephyr** (3.7.0 LTS and/or 4.4.0) **+** the
      nano-ros import (`integrations/zephyr/west.yml`), so `west init -m
      <template>` + `west update` yields a known-good (Zephyr × nano-ros) pair.
- [ ] An `apps/<app>/` skeleton — `CMakeLists.txt`, `prj.conf` (`CONFIG_NROS=y` +
      RMW), `src/main.c` (or a rust variant with the `rustapp` `[lib]` +
      `generate-config` note from Phase 202.4).
- [ ] A README mirroring the Phase 202 BYO flow: `nros setup zephyr --rmw …`
      (incl. `--source px4-rs`), `west patch apply`, `west build`, run.
- [ ] CI on the template repo that runs the quickstart on a fresh runner (proves
      it stays green as nano-ros / Zephyr move — the template is where
      Zephyr-version drift surfaces first).
- [ ] Link it from the book BYO page + `examples/README.md` as the recommended
      starting point (the 2026-05-04 UX study flagged the missing `west init`-style
      template).

**Files:** a new repo; `book/src/getting-started/integration-zephyr.md`,
`examples/README.md` (links). The in-repo `examples/templates/` may host a
mirror/source for the skeleton.

### 205.B — [P3] Upstream the generic patches (shrink the surface)
- [ ] **NSOS patches** (`recvmsg`, IPv4-multicast `setsockopt`/`getsockopt`
      forwarding) — generic Zephyr native-sim fixes, already flagged
      `upstreamable: true` in `zephyr/patches.yml`. Open Zephyr PRs; once merged +
      released, drop them from `patches.yml` for that Zephyr line.
- [ ] **`zephyr-lang-rust` patches** (`cargo-features` / `EXTRA_CARGO_ARGS`
      forwarding, the per-arch rust target registration) — pursue upstream so the
      rust examples need no in-tree patch (also removes the lang-rust-shape-drift
      fragility the Phase 202.5 version-tolerant patch papers over).
- [ ] **cyclonedds-on-Zephyr patches** — track upstream cyclonedds; they're
      currently baked into the nano-ros submodule pin.
- [ ] As each lands upstream, narrow the tested-version matrix note (fewer patches
      = less drift risk for 205.A's template).

**Files:** `zephyr/patches.yml`, `zephyr/patches/`, `scripts/zephyr/*-patch.sh`,
upstream PRs (human follow-up — this phase does not open PRs).

## Acceptance
- [ ] `west init -m <template>` + the README steps reach a running `native_sim`
      app on a fresh machine, CI-proven on the template repo (205.A).
- [ ] No pre-patched Zephyr is vendored anywhere; patches stay `west patch` /
      script-delivered to the user's own workspace.
- [ ] At least the NSOS patches have upstream Zephyr PRs open (205.B).

## Notes
- The starter-template is a *manifest+app* repo, deliberately **not** a Zephyr
  fork — keep it that way; the value is the pinned (Zephyr × nano-ros) pair + the
  documented quickstart, not vendored sources.
- Phase 202 already made the BYO module build self-contained (px4-rs provisioning,
  the `NROS_PLATFORM_CFFI_INCLUDE` cmake export); 205.A just packages that into a
  one-command start.
- Version-drift is the through-line: the template pins tested versions; CI on the
  template catches drift; 205.B reduces how much can drift.
