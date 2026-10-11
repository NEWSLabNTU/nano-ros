---
id: 1305
title: "Switching a single-package Rust leaf's board edits its Cargo.toml too — every image-only dependency is still hand-written (RFC-0098 D6 unmet)"
status: open
type: bug
area: tooling, cli, examples
severity: medium
found: 2026-09-11
related: [rfc-0098, rfc-0065, phase-445]
---

## 2026-10-12 (later) — the selection package lands; two leaves migrated (still OPEN)

The re-scoped fix is implemented, and proven on two leaves. The example sweep
(about 60 manifests) is NOT done yet, so the issue stays open.

**What landed.**

- `nros sync` and `nros build` write a single-package leaf's
  `<pkg>_nros_selection` package at `<leaf>/build/nros/selection/`
  (`leaf_settings::write_selection` → `facade::write_leaf_selection`). Sync
  writes it before its first cargo call, because cargo will not load a
  manifest whose path dependency is missing. The directory does not depend on
  the image or the board, so the leaf's dependency line survives a board
  switch. A leaf with several images gets the selection of the image being
  synced or built (in-tree, no leaf has two images on different boards).
- The package holds every image-only dependency, from the same code a
  generated workspace entry uses: `builder::entry::image_dependency_lines`
  (board crate with `default-features = false` and its features, the
  `nros-platform` platform feature, the descriptor's `crate_root_deps`) and
  `facade::derive` (edition, capability and RMW features, shared with
  `write_facade`). The `board-entry` / `rtic` entry feature is derived from
  the board key's framework (`entry::entry_surface_features`), so a generated
  workspace entry on `qemu-mps2-an385` now gets it too. Its `src/lib.rs`
  re-exports the board crate and carries the board's `crate_root_extra`
  (`use panic_semihosting as _;`), unless the image states a `panic` policy
  other than `own`.
- `nros::main!` reaches the board through the package when the leaf depends on
  `<pkg>_nros_selection` and does not name the board crate
  (`leaf_selection_crate`, `reroot_through`). The bare-metal reset vector now
  names `entry` and `cortex_m` through the board crate's re-exports, so a
  migrated leaf needs no `cortex-m` or `cortex-m-rt` of its own.
- The sync refusal (`check_board_crate_dependency`) skips a migrated leaf and
  still guards every leaf that names its board crate by hand.
- The mps2 descriptors' `crate_root_deps` now name `panic-semihosting` with
  `exit`, the feature every hand-written mps2 leaf named.

**Migrated:** `examples/mps2-an385-baremetal/rust/talker` (its manifest now
names `nros`, `log`, `std_msgs` and the selection package, and `main.rs` no
longer has `use panic_semihosting as _;`) and
`packages/testing/nros-tests/bins/entry-poc` (native).

**Acceptance, measured.** On a copy of the migrated talker outside the
checkout, the one-line edit `board = "qemu-mps2-an385"` → `"freertos"` gave
`nros sync` rc=0 and `nros build` rc=0, and `git diff` showed only that line.
In QEMU (slirp, `lan9118`, router `rmw_zenohd` on `tcp/127.0.0.1:10500`) the
image printed "nros FreeRTOS Platform", "Network ready." and "Application
setup complete", then published 27 messages in 30 s. The in-tree bare-metal
image published 29 in the same run shape. Mutation: with the selection writer
disabled, the same switch on a copy synced and built for the old board first
failed at `nros build` (rc=101,
`cannot find nros_board_mps2_an385_freertos in qemu_bsp_talker_nros_selection`).
With the writer restored, the same copy built (rc=0).

**Still owed (phase C, the sweep).** Most leaves are mechanical: drop the
board crate, the RMW backend, `cortex-m`, `cortex-m-rt`, the panic crate and a
platform-featured `nros-platform`, then add the selection dependency. These
need more than that:

- FreeRTOS, NuttX and ThreadX leaves that say `panic = "platform"` in
  `main.rs` have to state the same policy in their `[image.*]`. Otherwise the
  selection links the board's crate-root panic provider, and the image gets a
  second handler.
- The serial and XRCE mps2 leaves: the writer does not yet derive the board
  features `serial` (from `transport = "serial"`) or `xrce-transport` (from
  `rmw = "xrce"`).
- Native leaves with an `rmw-*` / edition feature axis: fixture rows select
  the RMW with cargo features. That has to become one image per RMW before the
  board can leave the manifest.
- RTIC leaves: `rtic` and the PAC stay in the leaf, because RTIC's expansion
  names `rtic::` and `device = mps2_an385_pac`. The board and RMW can move.
- C/C++ apps on a cargo link root: their own `main.rs` names `cortex_m_rt`.

Zephyr Rust leaves (west road) and the rv-virt-threadx staticlib leaves (cmake
road) are not on this road. When the sweep is done, the refusal can become
"no leaf names a board crate".

## 2026-10-12 — the switched image delivers; two edits, not three

The switch measured below was finished end to end. On a copy of
`examples/mps2-an385-baremetal/rust/talker`:

- `system.toml`: `[image.mps2-an385-baremetal] board = "freertos"`. The network
  identity (ip 10.0.2.10, locator `tcp/10.0.2.2:10500`) was left as it was, and
  the image boots with it, so the per-image overlay reaches the FreeRTOS board.
- Edit 1, the board crate: `nros-board-mps2-an385-freertos = { version = "*" }`.
- Edit 2: delete the leaf's `nros-rmw-zenoh` line.

`nros sync` and `nros build` both exit 0. In QEMU (default slirp,
`lan9118`) the image reaches "Application setup complete" and publishes 27
messages in 30 s. `ros2 topic echo --no-daemon /chatter` (Humble,
`rmw_zenoh_cpp`, router `just zenohd tcp/127.0.0.1:10500`) receives all 27. The
unmodified `examples/mps2-an385-freertos/rust/talker`, run the same way as a
control, also gets 27/27.

The possible third edit below is not needed. The FreeRTOS board crate enables
`nros-platform/platform-freertos` itself
(`packages/boards/nros-board-mps2-an385-freertos/Cargo.toml`), and adding the
leaf-side line leaves the image's section sizes identical (text 490064, data
4520, bss 933036). The reference example's comment claiming the line is
required was stale and now says so. So the fix this issue asks for has to
generate two dependencies plus the runtime crates, not three.

## 2026-10-11 (later) — the switch is not one edit, and the fix is wider than the board crate

The refusal above said "the one edit". That was measured the same day on a copy
of `examples/mps2-an385-baremetal/rust/talker` switched to `board = "freertos"`,
and it is false:

1. The literal edit (rename `nros-board-mps2-an385` to
   `nros-board-mps2-an385-freertos`, keeping its spec) still fails `nros build`
   (rc=101): `nros-board-mps2-an385-freertos does not have that feature`
   (`board-entry`; it has `default, rmw-zenoh, safety-e2e`).
2. With the feature dropped, the build stops in `zpico-sys`'s build script:
   `Only one platform backend can be selected at a time`. The leaf's
   `nros-rmw-zenoh = { features = ["platform-bare-metal", ...] }` belongs to the
   OLD board; the FreeRTOS board brings zenoh itself. Deleting it builds (rc=0),
   and the image boots in QEMU to "Network ready" (delivery not checked: the
   router was started as `just zenohd locator=…`, which passes the literal
   `locator=…` as the locator; issue 1818).
3. Possibly a third: `examples/mps2-an385-freertos/rust/talker` also names
   `nros-platform = { features = ["platform-freertos"] }`, with a comment that
   without it the app never reaches "Application setup complete". Not checked
   at runtime.

Left harmless but stale: `cortex-m`, `cortex-m-rt` (the board's own linker
script is used, not `link.x`).

So the gap is not "the board crate dependency" but **every image-only
dependency**: the board crate with that board's features, the RMW backend and
its platform feature, the `nros-platform` platform feature, and the runtime
crates (rt, panic). The refusal message now says so (it names the board crate
and points at the rest), instead of promising one edit.

**What the board descriptor does NOT carry.** `[board.entry] crate_root_deps`
is only the crate-root `use` items (`panic-semihosting` for both mps2 boards).
The workspace-entry generator (`builder/entry.rs`) builds the rest from
`board_crate` + `default-features = false` + the board's features, an explicit
`nros-platform` platform feature for every non-bare-metal board, and the RMW
from the selection facade. The bare-metal `nros-rmw-zenoh ... platform-bare-metal`
line comes from a scaffold template (`cargo-nano-ros/src/scaffold.rs`), not a
descriptor. So generating a leaf's dependencies needs the same inputs as
`entry.rs`, not `crate_root_deps` alone.

**Re-scoped fix (RFC-0098 amendment of 2026-10-11).** Neither original
candidate. The leaf keeps its own `Cargo.toml` and stays its own cargo root
(the documented `cargo build --manifest-path … --config …` flow), and its
target-scoped table names ONE board-agnostic path dependency: a generated
`<pkg>_nros_selection` package under `build/`, which the workspace entry
already uses (`facade.rs`). `nros sync` writes it per image with exactly what
`entry.rs` puts in a generated entry; `nros::main!` reaches the board through
it instead of naming `::nros_board_<x>`. Then a board switch is one line, and
the selection package is the only place image-only dependencies live (RFC-0103
layer 4: a package names no board, RMW or network). Size M-L. The dependency
stays target-scoped so the host probe still builds the lib (issue 1265).

Side finding, not this issue: `nros sync` printed `deploy-bound probe failed:
... failed to select a version for nros-board-mps2-an385-freertos` and still
exited 0. Issue 1817: sync now stops on a leaf whose own manifest does not
resolve, with cargo's whole error.

## 2026-10-11 — re-measured, and now refused up front (still OPEN)

Re-measured on main: still true. The same one-line switch of
`examples/mps2-an385-baremetal/rust/talker` to `board = "freertos"` gave
`nros sync` rc=0, then `nros build` rc=101 with
`cannot find nros_board_mps2_an385_freertos`.

**Interim:** `leaf_settings::check_board_crate_dependency` runs where sync and
build both pass (`leaf_settings::write`). It compares the crate the image's
board maps to (`entry::macro_board_crate`, the macro's own mapping) with what
the leaf's `Cargo.toml` names, including target-scoped tables. A mismatch now
fails at `nros sync` (rc=1) with a message naming both crates and the one edit.
A test walks every in-tree cargo leaf, so the refusal fires on no example.

**Still owed (why this stays open):** D6 itself. The board dependency should
be generated for a leaf as it is for a workspace entry, so the switch is one
line. Size L. A leaf is its own cargo root, and a `--config` file cannot add a
dependency.


# The claim

RFC-0098 D6: *"the board crate dependency is generated… No user manifest
names a board crate, so switching board touches no `Cargo.toml`."*
phase-445's acceptance: *"Switching a single-package example to another board
is ONE line in `system.toml`; `git diff` afterwards shows that line only."*

# What actually happens

Measured 2026-09-11, on a copy of `examples/mps2-an385-baremetal/rust/talker`
outside the checkout, with phase-445 W4b + W5 applied. One line changed in
`system.toml` and nothing else:

```diff
 [image.mps2-an385-baremetal]
-board = "qemu-mps2-an385"
+board = "freertos"
```

`nros sync` reports success. `nros build` then fails in the leaf's own crate:

```text
error[E0433]: cannot find `nros_board_mps2_an385_freertos` in the crate root
 --> src/main.rs:4:1
  |
4 | nros::main!(panic = "own");
  | ^^^^^^^^^^^^^^^^^^^^^^^^^^ could not find `nros_board_mps2_an385_freertos`
  |                            in the list of imported crates
```

# Why — two readers of "which board", and they disagree

D6 *is* implemented, for the generated workspace entry. A single-package leaf
is its own entry, and nothing generates its manifest, so the board reaches it
through two independent paths:

- `nros::main!` resolves the board the IMAGE names (phase-445 W5 made this
  exact: "resolves the declared board and nothing else"). It correctly asked
  for `nros_board_mps2_an385_freertos`.
- The leaf's `Cargo.toml` still hand-names the board crate —
  `nros-board-mps2-an385 = { version = "*", features = ["board-entry"] }` —
  and `nros sync` writes its `[patch.crates-io]` rows from what the MANIFEST
  names, so the generated settings file patched `nros-board-mps2-an385` while
  the macro asked for the other one.

Everything else followed the switch correctly, which is what makes this a
narrow gap rather than a design problem. The generated
`build/<image>/nros-cargo.toml` picked up the new board's whole toolchain:
the link group moved from `-Tlink.x` (cortex-m-rt's) to
`-Tmps2_an385.ld --nmagic`, `NROS_BOARD` became `freertos`, and the
`CC_thumbv7m_none_eabi` cross compiler came along. Only the crate dependency
did not.

# Consequence

- The acceptance sentence above is not met for the 70-odd single-package Rust
  leaves, and the failure is a `rustc` error naming a crate the user never
  wrote, rather than anything that says "also change the dependency".
- The docs have to say "two places" (`docs/design/0098-…` promises one).
  phase-445 W7 documents the true shape and links this issue.
- `nros sync` is the natural place to catch it and currently does not: it has
  both facts in hand — the image's board and the manifest's board crate — and
  reports `done.`

# Fix

Two candidates, and they are not equivalent:

1. **Generate the dependency.** The leaf's board crate row moves out of the
   authored `Cargo.toml` and into what `nros sync` writes, the way the
   generated workspace entry already gets it. This is what D6 says, and it
   makes the switch genuinely one line. It needs somewhere for a generated
   dependency to live for a package whose manifest the user owns — cargo has
   no `--config` hook for `[dependencies]`, so this probably means the
   single-package leaf gets a generated entry too, i.e. it stops being its own
   entry.
2. **Refuse, loudly, at sync.** Cheap and strictly an improvement: when the
   image's board crate is not among the manifest's dependencies, fail naming
   both spellings and the line to change. This does not meet D6; it converts a
   confusing `rustc` error into an actionable one.

(2) is worth doing regardless of whether (1) lands, because (1) is a shape
change and (2) is a check over facts sync already has.

Acceptance for either is a BUILD, not a gate (#393): flip one `board =` line
in a single-package Rust leaf, run `nros sync` + `nros build`, and get an
image — with `git diff` showing that one line.
