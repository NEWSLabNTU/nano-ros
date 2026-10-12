---
id: 1305
title: "Switching a single-package Rust leaf's board edits its Cargo.toml too — every image-only dependency is still hand-written (RFC-0098 D6 unmet)"
status: resolved
resolved: 2026-10-13
type: bug
area: tooling, cli, examples
severity: medium
found: 2026-09-11
related: [rfc-0098, rfc-0065, phase-445]
---

## 2026-10-13 (phase E) — the last classes; RESOLVED

Every single-package leaf ON A SUPPORTED BOARD that named a board crate by
hand now depends on its generated selection package (branch
`fix/1305-remaining-leaves`). 69 leaves are on it (was 40).

**The walk could not see 15 of them.** The gate
(`every_cargo_leaf_outside_the_open_classes_uses_its_selection_package`)
iterated `git ls-files '*system.toml'`, so a cargo leaf with NO `system.toml`
was never visited, and the first version of this section called the class
done while 15 `examples/native/rust/*` leaves with a hand-written `main`
still named `nros-board-linux` (issue 0196's shape: reach narrower than the
rule). The walk now also asks every tracked `[package]` manifest under
`examples/` that has no `system.toml`: naming a board crate makes it a stray
(`named_board_crates`). Run on the pre-migration tree it went red naming
exactly those 15 (rc=101).

Out of scope, and the walk says why WITHOUT naming a path for the platform:

- **unsupported board** — a leaf whose board crate `packages/boards/
  board-support.toml` marks `tier = "scaffold"` is skipped
  (`scaffold_board_crates`). Today that is the dormant ESP32 (issue 1525,
  code kept untouched): `examples/esp32-c3-baremetal/rust/{talker,listener}`
  still name `nros-board-esp32-qemu` and were NOT migrated. Mutation: that
  row set to `tier = 2` → the walk names both ESP32 leaves (rc=101);
  restored → rc=0. The walk also asserts the rule matched at least one leaf.
- **by shape** (`NOT_ON_THE_SELECTION`, each with its reason):
  `bins/in-place-subscriptions` (1340's witness), `bins/qemu-baremetal-main-e2e`
  and `bins/rtic-run-plan-e2e` (board code in the node), `fixtures/*`
  (harness-templated), `examples/rv-virt-threadx/rust/*` (libs a CMake image
  links), `log-arena-probe` (names no board).

Sweep (examples): `git grep -lE '^nros-board-' -- 'examples/**/Cargo.toml'`
prints the 6 rv-virt-threadx manifests (by shape) and the 2 ESP32 ones
(dormant) and nothing else.

**Not covered by the walk:** tracked `[package]` manifests under
`packages/testing/` with no `system.toml` that name a board crate (18
`nros-tests/bins/*`, 2 `nros-bench/*`, 3 under `fixtures/`). They are test
harness programs, not example leaves; whether they belong to this rule is
open.

**Templates** (`local-msg-package/src/rust_consumer`,
`multi-package-workspace/src/pkg_rust_publisher`; issue 1829 resolved). They
are workspace PACKAGES that are also leaves, and the workspace arm of `nros
sync` never wrote a selection: the first copy-out run failed at sync's
metadata probe (`failed to load manifest for dependency
`rust_consumer_nros_selection``). Sync's workspace arm now writes each such
package's selection (`leaf_settings::write_selection`) before its first cargo
call, as the single-package arm does. Each package `.gitignore` gained
`/build/`. Measured: `scripts/check-template-copy-out.sh local-msg-package
multi-package-workspace` rc=0 (2 and 3 artifacts; rc=1 with the migration
alone). The README's own flow on a copy outside the checkout (`nros sync`,
then plain `cargo build` in the package) rc=0 for both; each binary opens its
session against a router and ends `nros: application complete`, the same as a
copy built from the old manifests.

**mps2 C/C++ apps on a cargo link root** (the twelve
`mps2-an385-baremetal/{c,cpp}/*`). Their `main.rs` reaches the board's
re-exports through the selection package, the path `nros::main!` emits for a
Rust leaf (`use <pkg>_nros_selection::nros_board_mps2_an385::{Config, entry,
run_bare}`); it never named `cortex_m_rt` itself (`entry` was already the
board's re-export). The board crate, `nros-rmw-zenoh`, `cortex-m` and
`cortex-m-rt` left the manifests; each image states `panic = "platform"`
(`nros-c`'s `panic-platform` is the handler). `nros-c` keeps its
`platform-mps2-an385` arm: the C API's own per-board selection (issue 1512),
not a board crate. Measured: `fixtures-build.sh baremetal c` and `… cpp` rc=0
(12 rows); `test_qemu_baremetal_capi_e2e` 6/6 PASS (solo). Against images from
the old manifests: text +16 to +28 B, data −4 B, bss 0 to −16 B; still one
`rust_begin_unwind`, no `panic_semihosting`.

**15 native leaves with a hand-written `main`** (`action-client-async`,
`action-client-rtic`, `action-server-rtic`, `custom-msg`,
`custom-transport-{talker,listener}`, `lifecycle-node`, `listener-rtic`,
`serial-{talker,listener}`, `service-client-async`,
`service-client-callback`, `service-client-rtic`, `service-server-rtic`,
`talker-rtic`). Each gained a `system.toml` (board `native`, no
`[[component]]` — they open their own `Executor`); `main.rs` calls
`<pkg>_nros_selection::nros_board_linux::register_linked_rmw()`. The seven
that had an `rmw-*` feature axis declare `native`, `native_xrce` and
`native_cyclonedds` images, as phase D's six do; the serial pair declares
`rmw = "xrce"`; `lifecycle-node` declares `features = ["lifecycle"]` (a
registry capability, lowered to `nros/lifecycle-services`). The `rmw-*`
features, `ros-humble` and the backend deps left the manifests;
`custom-transport-*` keep `nros-rmw-zenoh` only for `link-custom` (not a
registry capability), and every leaf keeps `nros-platform-cffi
[posix-c-port]`, which its `main.rs` uses directly. The duplicate
`service-client-callback` `rmw-zenoh` fixture row went (its resolver now takes
the plain row). Measured: `fixtures-build.sh linux rust` rc=0 (75 rows); `nm`
on each of the 15 artifacts: 13 link zenoh only, the serial pair XRCE only, no
cyclone; `lifecycle-node` carries the lifecycle services. Tests: `nano2nano`
`test_rtic_pattern_{communication,service,action}`, `custom_transport_loopback`
(`--features rmw`), `native_async_roundtrip_e2e` (2), `custom_msg` (7), `xrce`
`test_xrce_serial_{talker,listener}_starts` + `test_xrce_serial_communication`,
`interop_e2e` `zenoh_lifecycle_full_cycle`, and
`native_api` `test_service_callback_interop_rust_client_{c,cpp}_server`
(CALLBACK_RESULT) — all PASS.

**The gate.** Exemptions for the migrated classes are gone; floor 69.
Mutations, each confirmed applied: `nros-board-linux` re-added beside
`talker-rtic`'s selection → walk rc=101 and `nros sync` rc=1 ("depends on its
generated selection package … AND names the board crate"); `nros-board-mps2-an385`
beside the C talker's → rc=101 and sync rc=1; a template's old manifest
restored → rc=101 naming it; restored → rc=0 each time.

**Docs.** Every page that still said a single-package Rust leaf names its board
crate (the README, ten book pages, `docs/guides/creating-examples.md`,
`examples/README.md`, and the generated example READMEs) now says the board
line is the whole switch, on a supported board. The ESP32 guide and the ESP32
example READMEs are unchanged in substance (dormant; the guide's link now
points here).

## 2026-10-12 (phase D) — the native leaves: one image per RMW × edition (still OPEN)

The six `examples/native/rust/{talker,listener,service-*,action-*}` leaves are
on the selection package (branch `fix/1305-native-images`). Their RMW and ROS
EDITION are now declared per image (RFC-0031, and RFC-0056 as amended): each
declares `native` (zenoh, humble, `default_images`), `native_xrce`,
`native_cyclonedds`, and the three again for `iron` and `jazzy`. Their
`rmw-*` / `ros-*` / `default` features and the backend optional deps are
gone; `link-tls` (and the listener's `unstable-zenoh-api`) stay leaf features
over an optional `nros-rmw-zenoh`, because the capability registry
(`capability_resolver::CAPABILITIES`) does not declare them.

What it took:

- **`[image.<id>] ros_edition` / `[image_defaults] ros_edition`.** The fields
  already parsed (`ImageBlock`); nothing read them. `SystemToml::
  image_ros_edition` resolves image > defaults > system > humble, a typo at
  any rung a hard error naming its table. `facade::derive` takes the resolved
  edition (one writer: `write_facade` and `write_leaf_selection`), and puts it
  on the BOARD crate as well as the umbrella — `nros-board-linux` gained
  `ros-{humble,iron,jazzy}` forwarding to its backends, because the zenoh
  keyexpr gates the RIHS01 tail on `nros-rmw-zenoh`'s own feature and
  `nros/ros-jazzy` reaches only `nros-node`. rlm's `system_config` parser has
  no top-level `deny_unknown_fields` and ignores `[image*]`; nano-ros's own
  `SystemToml` / `ImageBlock` already declared the key, and
  `orchestration-ir::leaf_system` walks `toml::Value`, so no external schema
  had to move.
- **A cargo leaf with several images.** `nros sync --image <id>` (selection,
  `build/<id>/nros-cargo.toml`, and message codegen for that image's edition),
  `nros build <id>` (one image per invocation: the images share one selection
  dir and one `generated/`), `nros ws leaf-system --image <id>
  [--write-settings]`. The settings file names its image in `NROS_IMAGE`, and
  `nros::main!` reads that image (`leaf_system::read_image`). Sync records the
  edition `generated/` holds (`generated/.nros-ros-edition`); `nros build <id>`
  refuses an image of another edition, naming `nros sync --image <id>`.
- **Fixture rows.** The 12 xrce/cyclonedds rows say `image = "native_<rmw>"`;
  the six explicit `rmw-zenoh` rows were duplicates of the plain rows and are
  gone. The image rides the row's cargo args as `--nros-image=<id>`, so the
  group key (`nros_fixture_group_slug`), the build and the staleness probe
  separate two images by the computation that already separated two feature
  sets; `leaf-settings.sh` is its one reader. The test side's `Selector`
  gained `image`; `groups::select_rmw_row` answers `build_example_rmw` from the
  row's COORDINATE for a leaf that selects by image.
- **The same-leaf race.** Every row of a settings-road leaf writes its image's
  selection and settings (`ws leaf-system --write-settings`) and builds INSIDE
  `nros_leaf_lock` (`flock` on `<leaf>/build/nros/.leaf.lock`); the probe takes
  the same lock. Rows of different leaves still run in parallel. Measured on
  `fixtures-build.sh linux rust` (rc=0, 76 rows, make -j): every one of the 22
  rows of the six leaves carries its own backend — the 6 plain + 4 link-tls/
  multicast rows zenoh symbols only, the 6 `native_xrce` rows `uxr_` only, the
  6 `native_cyclonedds` rows `dds_create*` only.
- **`std` on the host board's backends.** The leaves used to add `std` on their
  own `nros-rmw-zenoh`; without it the backend's `log` output vanished and
  `nano2nano`'s GID / sequence / peer-mode / TLS tests read nothing. The board
  is hosted by construction, so it carries `std` now.
- **`just ros_editions build-e2e-fixtures`** builds images, not features
  (`native_<rmw>_<edition>`); jazzy/zenoh measured rc=0, six binaries, each
  with a `RIHS01_` hash and no `TypeHashNotSupported`.

**`bins/in-place-subscriptions` stays exempt, with the cause measured.** It is
not a writer bug: `entry::image_dependency_lines` gives the host board crate
`census` + `profile` on EVERY road (issue 1419, phase-463 W7 — the generated
host entry is a census and profile producer), and the hand-written manifest
lacks them. Built both ways and compared with `just mem-report`:
`nros::metadata_mode::state::STATE` +75,992 B and `EXECUTOR_BACKING` 20,864 →
45,440 B (the 34,816-byte arena). Migrating would change the image issue 1340
measures. The eight `matched no recorded subscription row` warnings were a
second, runtime defect: `metadata_mode::begin_node` armed the registration
observer on every boot of a `census` image, census run or not, so EVERY
generated host entry (and now the native listener) logged one per
subscription. The observer now WARNS only in a census run
(`census_hooks::recording_run`); the join is still attempted.

**What is left (why this stays open).** Single-package cargo leaves still
naming a board crate by hand:

- `examples/templates/local-msg-package/src/rust_consumer` and
  `examples/templates/multi-package-workspace/src/pkg_rust_publisher`
  (`nros-board-linux`) — waiting on the copy-out lane (issue 1829);
- the dormant `examples/esp32-c3-baremetal/rust/{talker,listener}` (issue
  1525);
- the twelve `examples/mps2-an385-baremetal/{c,cpp}/*` C/C++ apps on a cargo
  link root, whose own `main.rs` names `cortex_m_rt`.

Out of scope by shape, not waiting: `bins/qemu-baremetal-main-e2e` and
`bins/rtic-run-plan-e2e` call `nros_board_mps2_an385::exit_success()` (board
code, not a board choice), `fixtures/*` are harness-templated, the
rv-virt-threadx staticlibs are linked by a CMake image, `log-arena-probe`
names no board (issue 1815), and `bins/in-place-subscriptions` above.

## 2026-10-12 (phase C) — the sweep: 34 leaves on the selection package (still OPEN)

Phase C moved every leaf in the mechanical classes, one commit per family
(branch `fix/1305-phase-c-sweep`). Each family was built with `nros sync &&
nros build` in every leaf and with its `fixtures-build.sh` lane, then
smoke-run:

| family | leaves | fixtures | smoke |
| --- | --- | --- | --- |
| mps2 bare-metal | listener, serial-talker, serial-listener, talker-xrce | `baremetal rust` rc=0, 21 rows | talker + listener in QEMU, router on 10500: 29 published, 27 "I heard" |
| mps2 FreeRTOS | all six | `freertos rust` rc=0, 8 rows | talker + listener, router on 7800: "Application setup complete", 27 published, 25 heard |
| NuttX | all six + `bins/param-store-nuttx-qemu-arm` | `nuttx rust` rc=0, 8 rows | NOT green: `rtos_e2e` pubsub fails with `Executor::open failed: Transport(ConnectionFailed)` for the migrated images AND for images rebuilt from the unmigrated manifests, so not caused by this |
| ThreadX | the six threadx-linux leaves + `bins/param-store-threadx-riscv64` | `threadx-linux rust` rc=0, 8 rows; riscv64 param-store row rc=0 | threadx-linux pair, router on 9000: 19 published, 19 heard; `param_store_carve` riscv64 PASS |
| RTIC | the eight `*-rtic*` leaves | `baremetal rust` rc=0, 21 rows | talker-rtic + listener-rtic, router on 10200: 29 published, 27 heard |

What the sweep needed besides the mechanical edit:

- **`panic = "platform"` in the image.** The 20 FreeRTOS, NuttX and ThreadX
  leaves whose `main.rs` says so now state it in their `[image.*]`, so the
  selection does not also link the board's crate-root panic provider.
- **Board features from the image.** `builder::entry::image_link_features`
  derives `serial` from `transport = "serial"` (and drops the IP link feature
  `ethernet`, which the mps2 crate refuses beside it) and `xrce-transport`
  from `rmw = "xrce"` over that link. Only features the crate declares are
  named. Unit-tested, with a tripwire that the mps2 crate still declares the
  three literals. Applied to a leaf's selection only: a workspace entry takes
  the board's defaults from its facade, which knows no image, and no in-tree
  workspace image declares a serial transport.
- **RTIC keeps `rtic` and the PAC.** The board crate (`rtic` feature, derived
  from the board key), the RMW backend, `cortex-m` and the panic crate moved.
- **Test bins keep their own `nros-platform` features** (`alloc-stats`, and
  on riscv64 `global-allocator` / `critical-section`), minus the platform
  feature. The riscv64 bin's DCE anchor names the selection package.

**The gate.** `check_board_crate_dependency` now refuses a leaf that depends
on its selection package AND names a board crate. A new test,
`every_cargo_leaf_outside_the_open_classes_uses_its_selection_package`, walks
every tracked cargo-road leaf: each must depend on its selection package or
match `leaf_settings::NOT_ON_THE_SELECTION`. An exemption that matches no
leaf fails, so the list can only shrink. Mutation: adding
`nros-board-mps2-an385-freertos` back to the migrated FreeRTOS talker gave
`nros sync` rc=1 and the test module rc=101; restoring the threadx-linux
talker's old manifest failed the walk, naming that leaf.

**What is left (why this stays open).** `NOT_ON_THE_SELECTION`, with reasons:

- **native examples** (`examples/native/rust/{talker,listener,service-*,action-*}`)
  — in scope by RFC-0031 (the RMW is `[image] rmw`; the cargo feature is the
  lowering target) and RFC-0056 (the edition is `[system] ros_edition`), and
  `fixtures.toml` rows already take `image = "<id>"`. The RMW axis alone could
  become one image per RMW. They stopped on the EDITION: `just ros_editions
  build-e2e-fixtures` builds all six with `--features "rmw-<x> ros-<edition>"`
  for iron and jazzy, while the selection package derives one edition from
  `[system] ros_edition` — a leaf that needs two editions has no spelling here
  without inventing a per-image edition. Two more things a migration has to
  solve, measured on the way: the selection is ONE directory per leaf, so two
  fixture rows of the same leaf naming different images race on it (make runs
  rows in parallel); and the row group key (`nros_fixture_target_dir_flag`)
  and the test-side row selector (`FixtureVariant::rmw`) are both keyed on
  cargo features, not on an image.
- **`bins/in-place-subscriptions`** — issue 1340's measurement witness. Moved
  onto the selection, it built and ran, but its eight subscriptions logged
  "matched no recorded subscription row; its registration path stays
  refused" and the arena reported 34,816 bytes; the hand-written manifest
  logs neither. Reverted until that is explained (the selection's
  `nros-board-linux` carries `census` and `profile`, issue 1419, which the
  hand-written manifest did not).
- **templates** (`examples/templates/*`) — the copy-out lane is red (issue
  1829), so a migrated template cannot be proven outside the checkout.
- **board-specific test fixtures** — `bins/qemu-baremetal-main-e2e` and
  `bins/rtic-run-plan-e2e` call `nros_board_mps2_an385::exit_success()` from
  the node; `fixtures/*` use harness-templated manifests or a custom `main`.
- **C/C++ apps on a cargo link root** (`mps2-an385-baremetal/{c,cpp}/*`) —
  their own `main.rs` names `cortex_m_rt`.
- **rv-virt-threadx** Rust staticlibs (a CMake image links them) and the
  dormant ESP32 leaves (issue 1525: no fixture row).
- `log-arena-probe` names no board crate at all (issue 1815) and needs nothing.

Zephyr Rust leaves are on the west road and are not cargo-road leaves.

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
