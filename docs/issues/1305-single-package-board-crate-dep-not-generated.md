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

## 2026-10-12 (sweep) — 29 more leaves migrated; 4 families left, each for a stated reason (still OPEN)

31 single-package Rust leaves now name only `<pkg>_nros_selection`:

| family | leaves | scope | generator change | verified |
| --- | --- | --- | --- | --- |
| mps2 bare-metal (`qemu-mps2-an385`) | listener, serial-talker, serial-listener, talker-xrce (+ pilot talker) | `target_os = "none"` | `transport_features`; board `rmw-xrce` | sync+build 4/4; `fixtures-build.sh baremetal rust` rc=0 |
| mps2 RTIC (`rtic-mps2-an385`) | 8 `*-rtic*` | `target_os = "none"` | own `[[board]]` row; macro routes `#[rtic::app]`, `device`, consumer | sync+build 8/8; talker-rtic in QEMU: 29 published / 30 s |
| FreeRTOS mps2 (`freertos`) | 5 (+ pilot talker) | `target_os = "none"` | none | sync+build 5/5; `fixtures-build.sh freertos rust zenoh` rc=0; service pair in QEMU: `Result of add_two_ints: 5` |
| NuttX (`nuttx`) | 6 | `target_os = "nuttx"` | none | sync+build 6/6; `fixtures-build.sh nuttx rust` rc=0; talker size identical to an unmigrated copy (text 708600); boots to `Executor::open`, which fails `ConnectionFailed` for the unmigrated copy too (host networking) |
| ThreadX Linux (`threadx-linux`) | 6 | UNSCOPED (host image) | none | sync+build 6/6; `fixtures-build.sh threadx-linux rust` rc=0; talker: 19 published / 20 s |

**What the generator gained.**

- *The link follows the image's `transport`.* `transport_features` in a board
  descriptor (mps2: `serial` -> `serial`, `tcp`/`udp` -> `ethernet`). The
  selection package drops every listed link from the board's defaults and
  adds the declared one; a transport the board has no link for is refused.
  The serial leaves had written `default-features = false, features =
  ["serial"]` by hand.
- *XRCE is the board's `rmw-xrce`.* `nros-board-mps2-an385` gains
  `rmw-xrce = ["xrce-transport"]`, the selector the facade already derives
  from `rmw = "xrce"`; on this board XRCE rides the UART custom transport.
- *RTIC is its own `[[board]]` row* on the same crate: `board_features =
  ["rtic"]`, crate-root crates `rtic` / `cortex-m` / `panic-semihosting`. The
  board crate re-exports its PAC under `rtic`; the macro's RTIC `device` is
  that re-export, and in a selection leaf `nros::main!` routes `device`, the
  dispatch consumer and `#[rtic::app]` through the package and brings `rtic`
  into scope at the crate root (the expansion spells `rtic::export` relative).

**Scoping (issue 1265).** A cross image scopes the table to its own target OS.
A host-built image cannot: any cfg matching the image matches the host probe,
so ThreadX Linux names it unscoped, and the probe builds exactly the graph it
built before (the board crate and `nros-platform` it replaces were unscoped).
So a board switch across target OSes (FreeRTOS -> NuttX) also edits the table
header; within one target OS it is the `system.toml` line alone.

**Not migrated, and why** (the refusal still covers each):

- **`native` (6 leaves with a `system.toml`).** Their RMW is a CARGO FEATURE,
  chosen per fixture row (`no_default_features = true, features =
  ["rmw-xrce"]`, `["rmw-cyclonedds"]`, plus `link-tls` and an env-only peer
  row). The selection package derives one RMW from `system.toml`, so these
  rows must first become images (`[image.native-xrce]` …) that the fixture
  builder and the test locators select — a fixtures-manifest change, not a
  generator one. The other 14 `examples/native/rust/*` leaves have no
  `system.toml`, so they are not images and the refusal never reads them.
- **`rv-virt-threadx` (6).** No `nros::main!`: the entry is the board's
  `app_main!(crate::register)` in the leaf's own `src/app_main.rs`, and the
  crate is also a `staticlib` the CMake Cyclone road links with `--features
  rmw-cyclonedds,alloc` (six `rmw = "cyclonedds"` fixture rows). Moving the
  board crate behind the package needs that entry to become `nros::main!` and
  the Cyclone rows to become images.
- **Workspace templates (2: `local-msg-package/src/rust_consumer`,
  `multi-package-workspace/src/pkg_rust_publisher`).** Form-1 packages inside
  a workspace, so they take the workspace sync road, which writes the
  `[patch]` row but not the package. Not attempted further: on this host the
  UNMODIFIED `multi-package-workspace` already fails `nros sync` (the metadata
  probe: `no matching package named nros found`), so a migration could not be
  verified.
- **`esp32-c3-baremetal` (2).** ESP32 is dormant (issue 1525): no lane builds,
  tests or provisions it, so a migration could not be verified, and whether
  `esp_app_desc!()` survives as a library item stays unmeasured.

Out of scope here: the 12 C/C++ `mps2-an385-baremetal` leaves. They are
Rust-ROOTED (issue 1512) and their shared `main.rs` calls the board's API
(`run_bare`, `Config`, `#[entry]`) directly, so the board is in their source,
not only their manifest; a selection package would move the line without
making the leaf board-agnostic.

## 2026-10-12 (later) — the generated selection package landed; two pilots migrated (still OPEN)

The fix the 2026-10-11 amendment describes is implemented, and two leaves use
it: `examples/mps2-an385-baremetal/rust/talker` and
`examples/mps2-an385-freertos/rust/talker`. Each names ONE image-only
dependency, the same for every board:

```toml
[target.'cfg(target_os = "none")'.dependencies]
qemu_bsp_talker_nros_selection = { version = "0.0.0" }
```

**What generates it.** `nros sync` (and `nros build`) write
`<leaf>/build/<image>/nros-selection/` with
`orchestration::facade::write_leaf_selection`. Its inputs are the ones a
generated workspace entry uses, through the same code: `builder::entry`'s
`board_crate_dep` / `platform_dep` (now shared with `render_manifest`) give the
board crate with the descriptor's `board_features`, and `nros-platform` with
the platform feature; `facade::derive_selection` (split out of `write_facade`)
gives the edition and capabilities on `nros` and the board's non-RMW defaults
plus one `rmw-*` per backend the image links. The board's `crate_root_deps` are
dependencies too, and its `lib.rs` re-exports the board crate and each of those
crates under its own name, then carries the board's `crate_root_extra`
(`use panic_semihosting as _;`), the lines a generated entry puts in `main.rs`.

**How the leaf reaches it.** The leaf names the package registry-style and a
per-image `[patch.crates-io]` row points it at that image's copy. A literal
`path =` would put one image's directory in the leaf's own manifest, and cargo
cannot `[patch]` a path source, so a multi-image leaf could not switch images
without editing the manifest. This is how the leaf already reached its other
in-repo crates (`nros = { version = "*" }`). The row is in the image's
`nros-cargo.toml` (for `nros build` and `cargo --config`) and in the leaf's own
gitignored `.cargo/config.toml` (`ws::write_patch_block`), because the metadata
probe resolves the leaf's manifest before sync writes the settings file, and
cargo resolves target-scoped dependencies for the host too.

**How the macro reaches the board.** `nros::main!` checks the leaf's manifest
for a dependency named `<package>_nros_selection` (Form 1 leaves only; a
generated workspace entry reaches its bringup through `launch =` and is
unchanged). When it is there, the board path becomes
`::<sel>::nros_board_<x>::<Board>` and the bare-metal Cortex-M entry's
`cortex_m_rt` / `cortex_m` paths go through the same package.

Two descriptor rows the generated dependency needed, both in
`packages/boards/nros-board-mps2-an385/nros-board.toml`: `board_features =
["board-entry"]` (the leaf used to name it), and `cortex-m` / `cortex-m-rt` in
`crate_root_deps` (the macro emits both for this board; a generated entry on
it lacked them too). `panic-semihosting` there now carries the leaf's `exit`
feature. A leaf on the package says `nros::main!(panic = "own")`, since its
panic handler is the board's crate-root item; the FreeRTOS pilot moved from
`"platform"` to `"own"`, as the generated FreeRTOS workspace image already
says.

**Acceptance, measured.** A copy of the baremetal talker's tracked files
(`tmp/accept/talker`, inside the checkout), with `[image.mps2-an385-baremetal]
board = "freertos"` the ONLY edit (its `Cargo.toml` byte-identical to the
example's, sha1 `b973fc5b`): `nros sync` rc=0, `nros build` rc=0. The selection
package names `nros-board-mps2-an385-freertos` (`rmw-zenoh`) and `nros-platform`
(`platform-freertos`). In QEMU (slirp, `lan9118`, `rmw_zenohd` on
`tcp/127.0.0.1:10500`) it prints the FreeRTOS banner, "Application setup
complete", and publishes 27 messages in 30 s. ROS 2 delivery was not
re-checked. Both pilots, unmodified, also sync, build and boot on their own
boards (29 and 27 messages in 30 s).

**Unmigrated leaves are unchanged.** A leaf that still names its board crate
gets no package, and `check_board_crate_dependency` still refuses a mismatch;
its message now names the selection-package line as the fix.

**Still owed (why this stays open):** the sweep. 57 other single-package Rust
leaves name a board crate, across `qemu-mps2-an385` (15 more), `rtic-mps2-an385`
(8), `native` (8), `nuttx` (6), `rv-virt-threadx` (6), `threadx-linux` (6),
`freertos` (5 more) and `esp32-c3-baremetal` (2). Several need more than the
pilots did: RTIC leaves select `rtic`, not `board-entry`; the XRCE leaves select
`xrce-transport` / serial; an `esp_app_desc!()` crate-root item may not survive
being moved into a library; and a `native` (or `threadx-linux`) image is built
FOR the host, so a `cfg(target_os = "none")` dependency never reaches it and
its selection package must be named unscoped, which the host probe then builds
too. Each family needs its own build, so the sweep was left for a follow-up.

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
