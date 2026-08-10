---
id: 374
title: "`nros setup native` source-builds zenohd and pulls a second rust toolchain, while the book promises prebuilt toolchains"
status: open
type: tech-debt
area: build
related: [rfc-0014, issue-0204, issue-0368, issue-0373, phase-345]
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
