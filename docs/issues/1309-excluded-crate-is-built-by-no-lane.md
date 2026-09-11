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
related: [1217, 1155, 0287, 0957, 1146, phase-450, phase-451]
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

## Progress (phase-451 W4, 2026-09-11) — three of nine closed, and the cost measured

**Three crates are workspace members now**, each verified against BOTH lanes
(`just check workspace-embedded` and `cargo clippy --all-targets -D warnings`):

| crate | what it needed |
| --- | --- |
| `packages/rmw/transport-callbacks` | member + `[package.metadata.nros] host-only = true`, so 0287's derived exclude keeps it out of the thumb lane |
| `packages/platform/nros-baremetal-common` | nothing — it built clean on the host as it stood |
| `packages/boards/nros-board-freertos` | a dead back-edge removed first, see below |

### The freertos case is the whole issue in one crate

It could not be a member because `cargo check` refused it outright:

```
error: cyclic package dependency: package `nros-board-freertos` depends on itself. Cycle:
  nros-board-freertos
    ... which satisfies path dependency `nros-board-freertos` of nros-board-mps2-an385-freertos
    ... which satisfies path dependency `nros-board-mps2-an385-freertos` of nros-board-freertos
```

The back-edge was `reference-mps2 = ["dep:nros-board-mps2-an385-freertos"]`, a
152.1.A convenience feature. Its only user — a re-export of the per-board free
`run` — was **retired by phase-313 W-freertos (#0243)**, which left a comment
saying exactly that and left the dependency in place. Nothing enabled the
feature; no `cfg` read it. Its entire remaining effect was to make a package
cycle that kept the crate out of every workspace, and therefore out of every
lane.

That is phase-451's own subject — a declaration whose only remaining effect is
on belief — except this one's effect was on COVERAGE.

**It had never been FORMATTED either.** `check-workspace-fmt` runs
`cargo +nightly fmt --check`, which reaches workspace members. Making the crate
a member produced a fmt diff in code nobody had touched — `add_freertos_includes(...)`
and a `glue.file(...)` call in `build.rs` were both wrapped in a shape rustfmt
does not produce. So "no lane builds it" understated the reach: no lane built
it, linted it, tested it or formatted it.

**What the coverage was worth: 8 latent `-D warnings` errors**, sitting in a
crate every FreeRTOS image links, invisible because nothing compiled it:

* `build.rs:118` — `then(|| 2048_usize)` → `then_some`.
* `src/entry.rs` ×5 — `b"net_poll\0".as_ptr()` and friends →
  `c"net_poll".as_ptr().cast::<u8>()`.
* `src/entry.rs:306` — a doc line beginning `+ netif wait`, read as an
  unindented markdown list item.
* `src/entry.rs:153` — **an orphaned doc block**. `1778ba8c0` (issue 1146's
  fix, three days earlier) inserted `report_stack_peak` between
  `app_task_entry_runtime`'s doc comment and the function, so the `# Safety`
  contract for a raw-pointer `extern "C"` entry point documented nothing at
  all. Reattached. No lane could have caught it and none did.

### What remains

* **`nros-board-threadx` and `nros-board-nuttx` have the SAME cycle** — optional
  deps on `nros-board-threadx-{linux,qemu-riscv64}` / `nros-board-nuttx-qemu`,
  which depend back. Unlike freertos these are not dead: `reference-qemu` has
  live `cfg` readers in `nros-board-nuttx/src/lib.rs` (`:83`, `:366`, `:446`),
  and `nros-board-nuttx-qemu/Cargo.toml:52` documents that it deliberately does
  NOT enable the feature. Untangling them is a real change, not a deletion.
  Both now carry the measured reason in
  `.config/workspace-exclude-reasons.txt` instead of the guess ("generic
  facade") this issue's first pass recorded.
* **The four cross-dep crates are blocked on a missing mirror.**
  `nros-platform-{mps2-an385,stm32f4,esp32-qemu}` and `nros-board-esp32-qemu`
  depend on `cortex-m` / `esp-hal`, so they cannot build for the host — and
  there is no derived way to say so. The host lane's exclusions are
  `HOST_UNCHECKABLE` in `just/check.just:36`: **a hand-written 8-crate string**,
  which is precisely the 20-line hand list that issue 0287 retired on the
  embedded side and nobody retired on this one. Until `host-only`'s mirror
  exists — call it `embedded-only`, derived the same way — these four cannot be
  members without breaking host clippy, and so stay unbuilt.
* **`nros-platform-stm32f4`'s three `#[test]`s still run nowhere.** They are the
  measurable cost of the bullet above, and they close when it does.

So the shape of the remaining work is one mechanism, not six crates: **make the
host lane's exclusion derived, the way the embedded lane's already is.** Then
every one of these is a member that declares which lane it cannot enter, and
"excluded" stops meaning "unbuilt".

