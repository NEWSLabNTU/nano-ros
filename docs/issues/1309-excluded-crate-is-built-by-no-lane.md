---
id: 1309
title: "A root-workspace `exclude` entry opts a crate out of BOTH lanes, so eight
  in-tree crates are compiled only as somebody else's dependency — and three unit
  tests in one of them have never run"
status: open
type: bug
area: [build, ci, testing]
severity: medium
found: 2026-09-11
related: [1217, 1155, 0287, 0957, phase-450, phase-451]
---

## What

`packages/rmw/transport-callbacks` was in the root `Cargo.toml`'s `exclude` list
with no reason of its own — that is [issue 1217](archived/1217-workspace-exclude-list-is-unaudited.md),
and phase-451 W3 promoted it to a member on the evidence that it builds clean on
the host.

That broke `check::workspace-all`, and the breakage is the finding:

```
error[E0463]: can't find crate for `std`
  --> packages/rmw/transport-callbacks/src/lib.rs:19:5
```

`check::workspace-embedded` clippies **every workspace member** for
`thumbv7em-none-eabihf`. This crate's factories are `std::net::TcpStream` and
`std::sync::Mutex` over `std::collections`, so it cannot build there — and
through cargo's workspace feature unification it would have leaked `std` into
every no_std member, which is [issue 0287](archived/0287-host-only-workspace-members-break-embedded-lane.md)'s
exact failure.

**0287 already solved this.** It replaced a 20-line hand-written exclude list
with a DERIVED one: a crate declares

```toml
[package.metadata.nros]
host-only = true
host-only-reason = "..."
```

and `scripts/build/host-only-members.sh` derives the `--exclude` flags. 24 crates
declare it today.

`transport-callbacks` never needed to, because it was kept out of the embedded
lane by not being in the workspace AT ALL. And that is the defect:

> **Exclusion is not "checked by the other lane". It is checked by NO lane.**

Not host clippy (`check::test-targets` is `--workspace`, and it was not a
member). Not embedded clippy (same reason). Not `cargo test`. For as long as it
has been excluded, nothing in this repo has compiled that crate except an
example leaf that happens to path-depend on it.

## Measured, 2026-09-11, on `origin/main` + phase-451 W3

Of the 128 `exclude` entries, **55 are under `packages/`**. By shape:

| shape | count | is it built? |
| --- | ---: | --- |
| own `[workspace]` table | 19 | yes — its own root is a build target |
| own tracked `Cargo.lock` | 16 | yes — built standalone by a recipe |
| neither | 20 | **see below** |

Of the 20 with neither, 11 are leaf packages of a nested fixture workspace
(`packages/testing/nros-tests/fixtures/*/`), built when the fixture is built.
That leaves **nine**:

* `packages/interfaces/rcl-interfaces`, `packages/interfaces/lifecycle-msgs` —
  metadata shells with a `[package]` table, no `src/` and no declared target.
  Nothing to build; their real crates are the generated ones underneath. Not a
  defect, but see "A second question" below.
* `packages/platform/nros-baremetal-common`
* `packages/platform/nros-platform-mps2-an385`
* `packages/platform/nros-platform-stm32f4`
* `packages/platform/nros-platform-esp32-qemu`
* `packages/boards/nros-board-esp32-qemu`
* `packages/boards/nros-board-freertos`
* `packages/boards/nros-board-threadx`
* `packages/boards/nros-board-nuttx`

Those eight are real crates with code. They are reached only as path
dependencies of example leaves and board bundles, which compile the LIBRARY.
Their own `--all-targets` lint and their own test targets are reached by
nothing.

### The concrete cost, already sitting there

`packages/platform/nros-platform-stm32f4/src/phy.rs:182` carries three
`#[test]` functions that have never run:

```rust
#[test] fn test_detect_lan8742a() { … }   // :185
#[test] fn test_detect_dp83848()  { … }   // :195
#[test] fn test_detect_unknown()  { … }   // :201
```

They are pure logic over `detect_phy_type(id1, id2)` — no hardware, no
emulator, nothing that would stop them running on the host in milliseconds.
They are unreachable because the crate is not in any workspace, so no `cargo
test` invocation in this repo can name them.

This is [issue 1155](1155-arch-flags-tests-in-no-lane.md) one crate over —
there, `nros-board-common`'s `arch_flags` tests run in no lane because
`test-unit` activates no feature; here the crate is not in a workspace at all.
Same verdict, different mechanism, which is why fixing 1155 will not fix this.

## Why nothing reports it

The two mechanisms answer the same question and only one of them is derived:

| question | mechanism | derived? |
| --- | --- | --- |
| may this crate be built for a thumb target? | `[package.metadata.nros] host-only` | **yes** (0287) |
| is this crate part of the workspace at all? | root `exclude` list | no — hand-written |

A crate kept out by the second never has to answer the first, and nothing
notices that it also stopped answering everything else. `check-workspace-exclude-list`
(phase-451 W3) now derives a REASON for every exclusion, which is a different
property: it asks "is this exclusion justified", not "does any lane compile this
crate".

## Suggested shape

Not a fix, a direction — the interesting decision is whether exclusion should
remain a way to opt out of both lanes at once.

1. A crate that is merely host-only should be a **member** declaring
   `host-only = true`, not an exclusion. That is what phase-451 W3 did for
   `transport-callbacks` (member + metadata: embedded lane derives the exclude,
   host lane now compiles and lints it, both green).
2. The eight above should each be assessed the same way: if the reason is "it
   cannot build for the host", that is `host-only`'s mirror and needs its own
   declared form; if the reason is a cross toolchain, a `[build] target` pin
   makes it derivable.
3. A gate: an excluded crate that is not a member of any workspace, has no
   tracked lock, and declares a Rust target, is a crate nothing builds. That set
   should be empty or enumerated with reasons.

## A second question this raised

`packages/interfaces/rcl-interfaces` and `lifecycle-msgs` carry a `[package]`
table, a version, dependencies and features, and declare **no target at all** —
no `src/`, no `[lib]`, no `[[bin]]`. Cargo accepts it, and phase-451 W3's gate
had to grow a rule for the shape. Whether a package that builds nothing should
carry a manifest that looks like it builds something is worth answering
separately; it is not obviously wrong, and it is not obviously intended.

## Adjacent, and probably one gate eventually

[Issue 0957](0957-format-blocked-by-unexcluded-workspace-leaf.md) is the
complementary hole: a leaf in NEITHER `members` nor `exclude`. Both are
questions about the same two lists being complete and meaningful, and both are
currently answered by different code. Worth folding together when either is
worked.
