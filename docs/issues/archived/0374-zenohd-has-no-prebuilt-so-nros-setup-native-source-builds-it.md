---
id: 374
title: "`nros setup native` source-builds zenohd and pulls a second rust toolchain, while the book promises prebuilt toolchains"
status: resolved
type: tech-debt
area: build
related: [rfc-0014, rfc-0075, issue-0204, issue-0368, issue-0373, issue-0653, issue-0654, phase-345, phase-362]
---

# `nros setup native` source-builds zenohd — the book promises prebuilt

> **UPDATE 2026-08-01 — the honesty half landed; the issue now tracks only the
> missing asset.** Directions 2, 3 and 4 below are done: `nros setup` announces
> source builds up front on both the board and the `--tool` path, and
> installation.md no longer promises unconditional prebuilts. What is still
> open is direction 1, and it is **not fixable in this repository** — it needs
> `1.7.2-nros2` release assets published on `NEWSLabNTU/nano-ros-sdk` (the
> build script `ci/nano-ros-sdk/scripts/build-zenohd.sh` already carries the
> required `--features zenoh/transport_serial`), after which `[tool.zenohd]`
> gets its `dist.<host>` rows back and the default board stops compiling zenoh
> on every user's machine. Until then the wait is announced rather than
> removed.

## Summary

installation.md:123-127 tells the first-time user:

> `nros setup` … ships **prebuilt toolchains per platform per RMW** — the
> cross-compiler, emulator, RMW host daemon, and any SDK sources a board needs
> are fetched from a pinned index

For the page's own headline board (`native`, default `--rmw zenoh`) that is not
what happens. `[tool.zenohd]` has **no `dist.<host>` row** — the index says so
itself at `nros-sdk-index.toml:60-65` (the 1.7.2-nros2 rebuild carries
`--features zenoh/transport_serial`, and the nano-ros-sdk assets for it were
never seeded) — so every host falls through to the source recipe:

```
$ nros setup native --rmw zenoh --dry-run
nros setup: native (rmw zenoh) needs 3 package(s):
  zenohd                 source build 1.7.2-nros2 (no prebuilt for linux-x86_64)
  zenoh-pico             source 1.7.2 — submodule …
  mbedtls                source 3.x — submodule …
```

## Cost, measured

Arch Linux, x86_64, warm network:

- `cargo install --path zenohd --locked` over the full zenoh 1.7.2 workspace —
  many minutes of compile, unattended.
- A **second rustup toolchain** is downloaded for it, because the zenoh checkout
  carries its own pin:
  `info: syncing channel updates for '1.85.0-x86_64-unknown-linux-gnu'`.
  The host now holds `stable` (the nano-ros pin) plus `1.85.0`. This is the
  second undeclared toolchain sync of the documented flow — the first is the
  nano-ros `rust-toolchain.toml` pin (stable + 4 components + 3 bare-metal
  targets) that any in-tree cargo invocation triggers.
- `~/.nros/sdk/zenohd` = **792 MB** after the run completed (source checkout +
  target dir retained alongside the installed `1.7.2-nros2/bin/zenohd`).

None of this is stated anywhere before the user runs the command. `--dry-run`
does say `source build … (no prebuilt for linux-x86_64)`, but the flow in the
book does not route the reader through `--dry-run`, and the phrase does not
convey minutes-and-hundreds-of-MB.

## Direction

Pick one, they are not exclusive:

1. **Seed the asset** — STILL OPEN, out-of-repo.
   `ci/nano-ros-sdk/scripts/build-zenohd.sh` already carries
   the `transport_serial` flag per the index comment; publishing the
   `1.7.2-nros2` release assets restores `dist.linux-x86_64` and makes the doc
   claim true for the default board.
2. **Make setup honest up front** — DONE. `nros setup` resolves the whole plan
   first and prints a `BUILDING FROM SOURCE: <names>` heads-up with the time and
   disk cost before the first fetch, on the board path and the `--tool` path
   alike (`warn_source_builds` / `source_build_names` in `cmd/setup.rs`,
   unit-tested).
3. **Reword installation.md** — DONE. The page now says prebuilt *where the
   index has a binary for your host, source-built otherwise*, shows the
   heads-up, and names zenohd as today's exception with the ~800 MB figure.
4. **DONE 2026-08-10 (phase-345 W4).** Source recipes now build with the
   WORKSPACE's pinned channel: the executor sets `RUSTUP_TOOLCHAIN` for the
   configure and install steps (`sdk_store.rs`), read from the workspace
   `rust-toolchain.toml`. A recipe that genuinely needs its own pin opts out
   with `respect_toolchain = true` on its `[tool.*.source]` — because forcing a
   stable channel onto a nightly-only crate would turn a working recipe into a
   compile error. An unreadable pin means no override, since guessing a
   toolchain is worse than the download it avoids.

   **Measured before implementing, as the phase required.** zenoh 1.7.2 pins
   `channel = "1.85.0"` while nano-ros pins `stable` (today 1.97.1) — 12 minor
   versions apart, so "does it even build?" was the blocker. It does:

   ```
   cargo metadata --locked                          -> ok
   cargo check -p zenohd --locked                   -> 0 errors
   cargo check … --features zenoh/transport_serial  -> 0 errors   (the recipe's own features)
   cargo install --path zenohd --locked --features zenoh/transport_serial
                                                    -> exit 0, 15 MB binary
   $ zenohd --version
   zenohd v790faad built with rustc 1.97.1
   ```

   The `cargo install` was run in full rather than stopping at `check`, because
   a type-check is not a release build plus a link. The binary reporting
   `rustc 1.97.1` is the direct evidence that the override reached the compiler.

   The heads-up text is corrected in the same change: it used to tell the user
   a pinning recipe "also makes rustup fetch that toolchain", which is now only
   true for a recipe that opted out.

   Note this host still carries the `1.85.0` toolchain that the old behaviour
   installed — the fix prevents the next one, it does not clean up the last.

## Resolved 2026-08-17 — by removal, not by shipping a prebuilt

phase-362 W4 retired the vendored router entirely (RFC-0075: `rmw_zenohd` ships
with `rmw_zenoh_cpp` and links the same `libzenohc.so` the RMW does, so it
cannot drift from it). The `third-party/zenoh/zenoh` submodule, the `zenohd`
build recipe and the SDK-store entry are gone, which removes this issue's
premise rather than satisfying its ask.

Measured on the current tree:

```
$ nros setup native --dry-run
nros setup: native (rmw zenoh) needs 2 package(s):
  zenoh-pico   source 1.7.2 — submodule …/zenoh-pico
  mbedtls      source 3.x   — submodule …/mbedtls
```

Two submodule checkouts. No zenohd, and therefore no second Rust toolchain
pulled in to build one — which was the whole complaint.

The acceptance this issue actually wanted ("`nros setup native` does not
source-build a router") holds. The one it asked for ("ship a zenohd prebuilt")
is moot: there is no nano-ros router to ship. A host without
`ros-<distro>-rmw-zenoh-cpp` now gets a named skip from the zenoh lanes rather
than a source build, which phase-362 accepted explicitly as a cost.

→ phase-362, RFC-0075, issue 0660 (the recipe callers that deletion left behind).

### The documentation debt this left, now paid

Closing on "the code changed" alone would have been wrong: phase-362 W5
updated `book/src/design/rmw.md` and stopped, so the **getting-started**
page — the one this issue is about — still described the retired binary.
Swept here:

* `book/src/getting-started/installation.md` — the source-build heads-up no
  longer names zenohd or the ~800 MB/second-toolchain cost; the provisioning
  table no longer claims `native` and `qemu-arm-freertos` install a router;
  the "first example" heads-up now says the zenoh router comes from ROS and
  points at `just native zenohd` instead of `nros sdk-path zenohd`.
* `book/src/reference/cli.md`, `book/src/user-guide/workflow.md` — "zenohd is
  the notable source build" replaced by the vendored submodules that actually
  are.
* `activate.sh` — the ROS-less banner said setup, codegen "and the first-node
  flows" need no ROS. Building still does not; running a multi-process zenoh
  example now does.

### What this uncovered, filed rather than folded in

* **[issue 0653](0653-ros-less-host-has-no-zenoh-router.md)** — RFC-0075
  accepted "a ROS-less host cannot run the zenoh interop lanes", but the
  consequence is not limited to interop: zenoh-pico is a client, so *any*
  two-process zenoh example needs the router. A ROS-less host has none. The
  honest documentation is done here; whether zenoh should stay the default
  RMW for such a host is a decision, not a doc edit.
* **[issue 0654](0654-zenohd-invocations-name-a-retired-binary.md)** — ~95
  files still say `zenohd --listen …`. `rmw_zenohd` ignores argv, so those
  flags are unread rather than rejected: a silent wrong-port hang.
