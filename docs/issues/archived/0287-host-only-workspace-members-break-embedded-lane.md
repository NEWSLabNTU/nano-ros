---
id: 287
title: "A host-only workspace member silently breaks the embedded clippy lane through cargo feature unification"
status: resolved
resolved_in: issue-0287-fix
type: tech-debt
area: build
---

## Finding (phase-308, 2026-07-26)

Adding `nros-rmw-metadata` — a host-only crate that deps `nros/metadata-mode`,
which implies `std` — broke `just check workspace-embedded` with:

```
error[E0463]: can't find crate for `std`
  --> packages/core/nros-serdes/src/lib.rs:31:1
   = note: the `thumbv7em-none-eabihf` target may not support the standard library
```

The crate itself is unreachable from firmware: its self-registration ctor is
`not(target_os = "none")`, and no board or entry deps it. But
`check-workspace-embedded` builds the whole workspace for a thumb target and
cargo unifies features across members, so `nros/std` turned on for everything.
Feature unification does not care what is reachable.

Fixed by adding the crate to the recipe's `--exclude` list, alongside the
other host-only members (`nros-orchestration-ir`, the build-script helpers,
the `-sys` crates).

## Fix (2026-07-31)

The exclude list is DERIVED, not hand-written. Each host-only crate declares
itself:

```toml
[package.metadata.nros]
host-only = true
host-only-reason = "bindgen + C build of zenoh-pico; needs a host toolchain"
```

and `scripts/build/host-only-members.sh` emits the `--exclude` flags. The reason
now lives next to the crate, and adding a host-only crate is one edit in the
file you are already editing.

There were **two** byte-identical copies of the list —
`check-workspace-embedded` and `build-workspace-embedded`. Both derive from the
one source now; a member added to one and not the other would have failed in
whichever lane was forgotten.

The lane's failure is also self-diagnosing: it prints a hint that the named
crate is a VICTIM of feature unification, not the cause, and shows the marker to
add. Verified by reproduction — unmarking `nros-rmw-metadata` drops it from the
derived list and the lane fails with the exact `E0463` above, hint attached.

The derived list was checked byte-identical to the hand-written one (20 of 20)
before the swap, so the mechanism changed and the behaviour did not.

## Why it is worth tracking

The exclude list is MANUAL and duplicated across two recipes in `justfile`.
Every future host-only crate hits this trap, and the failure points at
`nros-serdes` rather than at the crate that caused it — the diagnostic names
the victim, never the culprit. That is a long debugging session for whoever
adds the next one.

## Options

1. **Derive the exclusion.** Mark host-only crates declaratively — e.g. a
   `[package.metadata.nros] host_only = true` key — and have the recipe build
   the `--exclude` list from a workspace scan instead of a hand-maintained
   literal. Removes the duplication and makes the property live with the crate.
2. **Give host-only crates their own workspace**, as `packages/cli` already
   does. Cleanest isolation (cargo cannot unify across workspaces at all) but
   costs a second lockfile per group and complicates `cargo test --workspace`.
3. **Leave it, add a comment.** Cheapest; keeps the trap.

(1) is preferred: the information belongs on the crate, and the check that
needs it can then never be out of date.

## Re-check (2026-07-28): still live, and option 1 is under-specified

Verified against the tree while scoping issue 0288. Three corrections.

**The surface is bigger than "two recipes".** Seven recipes in `justfile` carry
`--exclude` lists (`build-workspace`, `build-workspace-embedded`,
`check-workspace`, `check-workspace-embedded`, `check-workspace-features`,
`check-stack-all`, `test-unit`). `nros-rmw-metadata` sits in two of them —
`build-workspace-embedded` and `check-workspace-embedded`.

**The predicted drift has NOT happened.** Those two embedded lanes carry **21
excludes each, identical, zero divergence**. So this is a maintenance surface
(42 entries that must stay in lockstep), not yet a live bug. Worth stating
plainly rather than leaving the impression of a fire.

**Option 1's `host_only = true` is under-specified**, and this is the real
finding. The 21 exclusions encode at least *four different reasons*, three of
them already documented in the justfile itself:

| reason | crates |
| --- | --- |
| needs native system headers for the CMake build | `zpico-sys`, `xrce-sys`, `cyclonedds-sys`, `nros-rmw-cyclonedds-sys` |
| requires `std` (test framework) | `nros-tests` |
| staticlib/cdylib needs platform-specific panic + runtime setup | `nros-c`, `nros-cpp`, the `*-staticlib` wrappers |
| host-only tooling | build helpers, `nros-orchestration-ir`, `nros-rmw-metadata`, `nros-board-{native,posix}` |

A single boolean flattens all four into one bit and discards the *why* — which
is this issue's actual complaint, that the diagnostic "names the victim, never
the culprit". If the property is made declarative it has to carry the reason so
the generated exclusion (and any failure message) can cite it:

```toml
[package.metadata.nros]
embedded = false
embedded-reason = "requires native system headers for the CMake build"
```

**Not the same fix as issue 0288**, despite the resemblance. The two axes are
opposite: this issue excludes a crate from the *embedded* build; 0288 excludes
one from the *host* probe. A crate can be in either set, both, or neither, so a
single key cannot express both.

Note also that option 2 (a separate workspace for host-only crates) only covers
the fourth row above. The `-sys` crates and `nros-c`/`nros-cpp` are genuinely
embedded crates excluded for build-shape reasons, and would stay on a manual
list regardless.
