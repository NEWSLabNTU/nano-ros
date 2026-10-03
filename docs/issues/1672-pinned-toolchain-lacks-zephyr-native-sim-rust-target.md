---
id: 1672
title: "Pinning the root toolchain to 1.99.0 dropped `x86_64-unknown-none`, so
  every Zephyr native_sim WORKSPACE Rust leaf fails with `can't find crate for
  core` — the triple was never declared anywhere the coverage gate reads"
status: open
type: bug
area: [build, zephyr, toolchain]
severity: high
found: 2026-10-03
related: [1447, 1153, 0833, 0944, 0196]
---

## Symptom

Found by the first `just build-test-fixtures lane=all` run on this host to reach
the build stage. The zephyr family fails on every native_sim workspace Rust leaf:

```
error[E0463]: can't find crate for `core`
error: could not compile `byteorder` (lib) due to 1 previous error
FAILED: [code=101] … build-ws-rs-entry-zenoh/rust/target/x86_64-unknown-none/debug/librustapp.a
```

`build-ws-rs-entry-zenoh` and `build-ws-rs-params-entry-zenoh` among others.

## Cause — three layers, each measured

**1. The triple was never declared.** Zephyr picks a board's Rust triple at
BUILD time (zephyr-lang-rust); for native_sim it is `x86_64-unknown-none`. It
was in none of `config/rust-targets.txt`, the root `rust-toolchain.toml`
`targets`, or the SDK index `[rust.target.*]` — `git log -S` finds it in the
root pin's history zero times. It reached hosts only through
`scripts/zephyr/setup.sh:321`, `rustup target add x86_64-unknown-none`, which
installs onto whatever toolchain is ACTIVE.

**2. Issue 1447 moved the active toolchain.** `187376c906` (2026-10-02) pinned
the root from `stable` to `1.99.0`. A workspace Zephyr leaf
(`examples/workspaces/*`) has no toolchain file of its own and resolves the root
one:

```
$ cd build/zephyr-workspace-builds/3.7/build-ws-rs-entry-zenoh/rust && rustup show active-toolchain
1.99.0-x86_64-unknown-linux-gnu (overridden by '<repo>/rust-toolchain.toml')
```

`rustup` installs exactly a toolchain file's listed targets on first use, so the
fresh `1.99.0` had all fourteen listed ones and not this one. On this host
`stable` had it (from `setup.sh`), `1.99.0` did not. The single-example leaves
under `examples/zephyr/` were unaffected: they resolve
`examples/zephyr/rust-toolchain.toml`, `nightly-2026-04-11`, which lists it.

**3. The coverage gate could not see it.** `check-rust-targets-covered` read five
declaring producers and reported `OK (16 listed, 15 declared …)` throughout. None
of the five names this triple — native_sim is Zephyr's board, not one with a
`nros-board.toml`, and nothing assigns it to `NROS_RUST_TARGET`. The one tracked
file that DID name it, `examples/zephyr/rust-toolchain.toml`, is a producer the
gate never read. Issue 0196's shape: a gate whose reach is narrower than the
rule, green while the build it guards is red.

## Fix

* `x86_64-unknown-none  rustup` in `config/rust-targets.txt`, mirrored into the
  root `rust-toolchain.toml` and the SDK index as `[rust.target.x86_64-none]` —
  the three places the gate holds in lockstep.
* The gate gains a SIXTH producer: the `targets` array of every tracked
  `rust-toolchain.toml` except the root one, which is the mirror and so cannot
  also be a source of rows.

## Verified

* **Reach, against the pre-fix lists:** with the new producer and `origin/main`'s
  three files, the gate exits 1 —
  `x86_64-unknown-none  declared by examples/zephyr/rust-toolchain.toml`. After:
  `OK (17 listed, 17 declared, 15 mirrored …)`.
* **Provisioning is now automatic:** with the pin listing it, `rustup` installed
  `x86_64-unknown-none` for `1.99.0` on first use — no manual `rustup target add`.
* **The failing fixture builds:** `ninja -C build/zephyr-workspace-builds/3.7/build-ws-rs-entry-zenoh`
  rc=0; 129 crates compiled for `x86_64-unknown-none`, and `librustapp.a`,
  `zephyr.elf` and `zephyr.exe` all freshly written by that run.

## NOT fixed here — why `just doctor` stayed green

The SDK index still declares `[rust.toolchain.stable] channel = "stable"`, and
its `[rust.target.*]` rows name no toolchain, so they default to that alias. So
`nros setup` installs targets onto `stable` and `nros setup --check` verifies
them there — while the root build toolchain is `1.99.0`. `187376c906` changed
`rust-toolchain.toml` and `tools/rust-toolchain.toml` only; nothing holds the
index's channel to the pin.

That is why this regression was invisible to the doctor, and it will hide the
next target gap the same way. It is left open here because the index is SHIPPED:
it drives `nros setup` for released toolchains, the channel string is asserted in
`nros-cli-core` tests (`rust_toolchain.rs`, `board_descriptor.rs`, `image.rs`),
and whether an installed toolchain should pin `1.99.0` too is issue 1447's call
to extend, not a side effect of fixing a missing triple.

## Acceptance

* [x] The triple has a row, is in the root pin, and is in the SDK index.
* [x] The gate reads the producer that declared it, and fails without the row.
* [x] A native_sim workspace Rust leaf builds on a host whose pinned toolchain
  was provisioned only from the toolchain file.
* [ ] The SDK index's toolchain alias follows the root pin, so `nros setup
  --check` verifies targets on the toolchain that builds — and a gate holds the
  two together.
