# Phase 451 — a dead build declaration that reads as authoritative

**Status (2026-09-11). W1–W3 landed the day the phase was opened; W4's
structural work is done and what remains is measured rather than blocked.** The
`embedded-only` mirror exists (issue 1315), the threadx and nuttx package cycles
are removed, and the four `cortex-m`/`esp-hal` crates turn out to be excluded by
an UPSTREAM feature exclusivity rather than by the missing mirror — which
refutes what issue 1309 assumed, and was found by implementing it. Each work item found
its subject understated: the dead cmake module could not have RUN had anything
included it (its `find_package` targets were deleted years apart), the dead
`cargo:` carriers were two of three rather than a stray, and the exclude list
needed SEVEN structural reasons where the issue named five — classifying against
five left 17 unjustified entries, not one.

## Why this phase exists

Three open issues report a build declaration that no longer does anything and
that a reader cannot tell is dead. They are small, and the reason to give them
one owner rather than three is that their cost is measured in reader time, which
is invisible per-site and adds up:

* [#1218](../issues/archived/1218-dead-nanoroslink-duplicate.md) —
  `packages/api/nros-c/cmake/NanoRosLink.cmake` defines the public verb
  `nano_ros_link_rmw` and nothing includes it. It **misled two of four
  independent readers in one session**, because it holds a fourth closed RMW
  list and a force-link that does not happen. It was already finding A1 of a
  codebase audit and had no tracked owner.
* [#1213](../issues/archived/1213-zpico-net-size-probe-publishes-to-a-retired-crate.md) —
  `probe_net_type_sizes` publishes `sizeof(_z_sys_net_socket_t)` /
  `sizeof(_z_sys_net_endpoint_t)` through three carriers, two of them aimed at
  `zpico-platform-shim`, a crate phase-129.D deleted. Its doc comment states the
  opposite of what the code does.
* [#1217](../issues/archived/1217-workspace-exclude-list-is-unaudited.md) — the root
  `Cargo.toml` carries **165 `exclude` entries, 36 of which name directories
  that do not exist**, and one host-buildable crate
  (`packages/rmw/transport-callbacks`) is excluded for no discoverable reason.
  (The issue says 57 members / 174 excludes; parsed as TOML on 2026-09-11 it is
  58 and 165. The issue counted quoted strings with a regex, which also catches
  the four wrapped comment lines quoting cargo's "current package believes it's
  in a workspace" error. The 36 is the same number either way.)

The pattern is one thing: **a declaration whose only remaining effect is on
belief.** A dead `exclude` line, a dead cmake module and a dead `cargo:` carrier
all compile, all pass every gate, and all answer a reader's question wrongly.

## What makes this worth a phase rather than three commits

Deleting each is a few minutes. Knowing each is dead is not — every one of these
was established by a whole-tree grep with exclusions, and #1218's cost was paid
four times over before anyone ran one. So each work item owes the same two
things: the evidence that the declaration is dead, and a way for the NEXT dead
one to be found without re-deriving it.

The exclude list is where that generalises: 36 of 165 entries naming absent
directories is not three mistakes, it is an unmaintained list, and a list nobody
maintains grows the fourth closed RMW list in #1218 all over again.

## Work items

### W1 — the dead `NanoRosLink.cmake` duplicate goes

[Issue 1218](../issues/archived/1218-dead-nanoroslink-duplicate.md). The live copy is
`cmake/NanoRosLink.cmake`, included by five platform modules as
`../NanoRosLink.cmake`. The `packages/api/nros-c/cmake/` copy is included by
nothing and is not installed.

- [x] The dead copy is deleted, with the grep that establishes it in the commit
      message.
- [x] The closed RMW list it carried is checked against the live one first — a
      fourth copy of a list is a fact about the list, and if the live one is
      missing something the dead one had, that is a finding, not debris.
      **It carried nothing:** the live `cmake/NanoRosLink.cmake` holds no closed
      list, resolving per-backend link data through `nros_rmw_dispatch()` and
      the R1 dispatch manifest, so the dead three-entry map was strictly
      narrower — it predates `uorb`.

**Landed TWICE on the same day, independently.** phase-444 W4.b deleted this
file too, reached from the RMW-agnostic side (its `check-rmw-agnostic` gate
counted the dead copy as the fourth closed backend list). Neither session knew
of the other; the rebase merged both resolution notes into issue 1218 rather
than picking one. That is not a process failure to tidy away — it is issue
1309's thesis with a second witness: nothing in this tree asks whether a
declaration is REACHABLE, so the same dead one can be found twice in one day by
two people looking for different things.

**What the deletion found.** The file was filed as a dead DUPLICATE, which
implies it would have worked. It would not have: `_nano_ros_rmw_targets` maps
`zenoh`/`xrce`/`cyclonedds` onto cmake packages `NrosRmwZenoh` / `NrosRmwXrce` /
`NrosRmwCyclonedds`, and none of the three exists in the tree —
`NrosRmwXrceConfig.cmake` was deleted by phase-140. Anything that had included
it would have failed at its first `find_package`. Its entire remaining effect
was on readers, which is why nothing but a reader ever reported it.
`packages/rmw/xrce/xrce-config.txt`'s comment, which named this file as the sole
namer of `NrosRmwXrce`, is corrected in the same change.

### W2 — the zpico size probe publishes only to a reader

[Issue 1213](../issues/archived/1213-zpico-net-size-probe-publishes-to-a-retired-crate.md).
Two of three carriers target a deleted crate; the doc comment describes the dead
path as the live one.

- [x] The dead carriers are removed and the comment describes what remains.
      Two, not one: the `DEP_ZPICO_*` pair on both the measured and the fallback
      path, and the `cargo:rustc-env=ZPICO_NET_SIZES_FILE` export.
- [x] The surviving carrier's reader is named in the comment, so the next
      deletion of that reader makes this dead loudly. The comment now says to
      delete the probe WITH its reader rather than leave a file written for
      nobody.

One behaviour deliberately left alone: on probe failure the function writes no
file, so the alias TU omits the defines and its `_Static_assert` is skipped —
documented at the read site as intentional. Writing the 16/8 fallback into the
file would have tidied the fallback into arming an assert against sizes nobody
measured. The `cargo:warning` now says what the missing file means instead of
naming the two variables it used to print.

### W3 — the root `exclude` list is audited and kept honest

[Issue 1217](../issues/archived/1217-workspace-exclude-list-is-unaudited.md). Every
legitimate exclusion in this tree satisfies one of five structural reasons — own
`[workspace]` table, own tracked `Cargo.lock`, a `.cargo/config.toml` pinning a
non-host `[build] target`, a cross-only dependency set, or "metadata only, no
Rust targets". Two entries fail that audit and 36 name nothing at all.

- [x] The 36 absent entries are removed — six contiguous runs of six, each with
      the comment that described only it. 165 entries to 128.
- [x] `packages/rmw/transport-callbacks` is a member. It builds clean on the
      host and `cargo metadata` resolves. `Cargo.lock` gains exactly one entry —
      the package itself plus its single dep `nros-rmw`, 7 lines — and no other
      package moves, which is the whole diff to review.
- [x] A gate: `just check workspace-exclude-list`
      (`scripts/check-workspace-exclude-list.py`, beside its sibling
      `nested-workspace-excludes` in `just/check/cargo.just`). Self-tests its
      classifier on every run; both failure arms mutation-tested.

**The five reasons were not enough, and finding that out is the work item's real
content.** Classifying all 129 surviving entries against the issue's five left
**17 unjustified** — not the one crate the issue names. A gate shipped on the
five would have been unlandable, and the tempting repair (widen the allowlist
until it passes) is how the list became unaudited in the first place. Two more
reasons are real and derivable:

* **an ANCESTOR directory carries the `[workspace]` table** — a member of a
  nested workspace. All 11 fixture leaf packages under
  `packages/testing/nros-tests/fixtures/*/`. This alone took 17 to 7.
* **the package declares no Rust target** — no `src/`, no `[lib]`, no `[[bin]]`.
  `packages/interfaces/rcl-interfaces` and `lifecycle-msgs`: metadata shells
  whose real crates are the generated ones underneath.

The remaining 4 — `nros-board-{freertos,threadx,nuttx}` and
`nros-baremetal-common` — are genuinely cross-only in a way no manifest fact
states: no cross-only dependency, no pinned target, because the kernel build
glue is not a cargo fact. They are DECLARED in
`.config/workspace-exclude-reasons.txt`, a shrink-only ratchet, rather than
given an invented derivation. A gate that asserts a reason it did not measure is
the defect [phase-450](phase-450-gate-reach-narrower-than-its-rule.md) exists
for, and this phase is not the place to add a sixteenth instance of it.

### W4 — an excluded crate is a crate no lane builds (issue 1309)

Opened by W3's own fallout, and it is the same defect one level up. Promoting
`packages/rmw/transport-callbacks` to a member broke `check::workspace-all`
(`can't find crate for std`), which revealed that exclusion is not "checked by
the other lane" but **checked by no lane at all**: not host clippy, not embedded
clippy, not `cargo test`.

[Issue 1309](../issues/1309-excluded-crate-is-built-by-no-lane.md) has the
measurement. Of 55 `packages/` exclusions, 20 have neither their own workspace
nor their own lock; 11 of those are fixture leaves and eight are real crates
reached only as somebody else's path dependency.

- [x] `transport-callbacks` — member + `host-only = true` (issue 0287's derived
      embedded exclude), both lanes green.
- [x] `nros-baremetal-common` — member; it built clean on the host as it stood.
- [x] `nros-board-freertos` — member, after removing the dead `reference-mps2`
      back-edge. That optional dep formed a package CYCLE, which is why cargo
      refused the crate anywhere; its only user was retired by phase-313 and the
      dep outlived it. **Entering the lanes surfaced 8 latent `-D warnings`
      errors**, including an orphaned `# Safety` doc block that `1778ba8c0`
      (issue 1146, three days earlier) had detached from a raw-pointer
      `extern "C"` entry point.
- [x] `nros-board-threadx` / `nros-board-nuttx` — both cycles REMOVED. The
      `cfg` readers turned out not to be an obstacle: nothing enabled
      `reference-qemu`, so `any(feature = "reference-qemu", target_os = "nuttx")`
      was already equivalent to `target_os = "nuttx"` in every build that
      exists, and dropping the disjunct at 12 sites is a no-op rather than a
      behaviour change. threadx's two features gated only re-exports from a
      transition that is over.
- [ ] Membership itself, which is no longer STRUCTURAL and is now measured:
      with the cycles gone both build clean on the host, and entering the host
      lane surfaces **20 latent `-D warnings` errors** (12 nuttx, 8 threadx) in
      crates nothing has ever compiled. That is what promotion costs.
- [x] The mirror exists: `[package.metadata.nros] embedded-only = true`, derived
      by `scripts/build/embedded-only-members.sh`, and `HOST_UNCHECKABLE` is
      derived from it. Five of its eight hand-written entries were STALE.
- [ ] ~~The four `cortex-m` / `esp-hal` crates — blocked on a missing mirror.~~
      **Refuted by doing it.** They are not waiting on the mirror: `cortex-m`,
      `esp-hal` and `nros-platform-critical-section` each select a different
      `critical-section` restore-state width, and critical-section refuses more
      than one outright. Upstream exclusivity, so no workspace build can hold
      them — tried as all four, as the cortex-m pair, and as the esp32 pair.
      `nros-platform-stm32f4`'s three `detect_phy_type` tests DID run while it
      was briefly a member (`3 passed`), so issue 1309's cost is measured now;
      reaching them permanently needs a per-crate test lane, not membership. The
      host lane's exclusions are `HOST_UNCHECKABLE` in `just/check.just:36`, a
      hand-written string: the 20-line hand list 0287 retired on the embedded
      side and nobody retired on this one. `nros-platform-stm32f4`'s three
      `#[test]`s over `detect_phy_type` stay unreachable until it is derived.

The remaining work is one mechanism, not six crates: make the host lane's
exclusion derived the way the embedded lane's already is, so "excluded" stops
meaning "unbuilt".

## Acceptance for the phase

* `grep`-reachable: no cmake module defining a public `nano_ros_*` verb is
  unreachable from any `include()`.
* The root `exclude` list is machine-checked, and the check fails on a
  deliberately added stale entry.

## Non-goals

* A tree-wide dead-code sweep. These three were filed; the periodic audit
  ([docs/development/codebase-audit-checklist.md](../development/codebase-audit-checklist.md))
  owns finding more.
* Anything about what the RMW lists should CONTAIN — that is
  [phase-444](phase-444-rmw-fix-up.md). W1 only checks the dead copy against the
  live one before deleting it.
