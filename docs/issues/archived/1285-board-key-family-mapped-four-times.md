---
id: 1285
title: "The board key -> board family mapping is spelled four times, and the four
  disagree"
status: resolved
type: tech-debt
area: cli, codegen
severity: low
resolved_in: "fix(#1285): one board key -> family table; an unknown key is an error"
related: [issue-1283, issue-1286, phase-432, rfc-0091]
---

## What happens

RFC-0091 §8b Defect 1, as written, is fixed. It was a C++ path spelling in a
view every pack shared. `LoweredEntry` now carries only the board key, and each
language's spelling lives in its own pack: C++ `board_cpp_path`, Rust
`board_path_for`, C `c_runner_names`.

The fact underneath, "which family is this board key?", is still derived four
times:

| site | how | unknown key |
| --- | --- | --- |
| `nros-entry-lower` `board_family` | match on key | falls to `Native` |
| Rust pack `board_path_for` | its own key set | refuses (proc-macro) |
| `nros-cli-core` `codegen/entry/mod.rs` `board_to_rtos` | SUBSTRING match | `posix` |
| `known_boards_csv` | its own list | n/a |

They disagree. `board_to_rtos` sends `s32z270`, `an536`, `armfvp` and
`fvp-aemv8r-smp` to `posix`, and `board_family`'s fallback turns ANY unknown key
into `Native`. That makes an embedded key a C entry does not know look like a
host build.

Second defect on the same seam: `emit_c.rs`'s `c_runner_names` has a ThreadX arm
naming two runner symbols that do not exist, and nothing ties the table to
`BoardFamily::has_c_run_components()`. The table and the routing predicate can
disagree, and today they only agree by accident.

## Why it is latent, not live

Audited 2026-09-11: no in-tree C or C++ entry passes a key the four disagree on.
- CMake (`NanoRosEntry.cmake` around lines 180–200) passes only `native` or
  `zephyr`.
- The multi_pkg fixtures with keys `board_family` does not know are Rust-only
  (`freertos`, which the proc-macro refuses on an unknown key) or declare no
  entry (`nuttx`, `esp_idf`).

So nothing is wrong today. It becomes wrong the first time someone copies a Rust
board key into a C workspace.

## Fix

- ONE table, key -> `BoardFamily`. `board_family`, `board_to_rtos` and
  `known_boards_csv` derive from it. The Rust pack's key set is checked against
  it, or derives from it too.
- An unknown key is an ERROR naming the known keys, never `Native`.
- `BoardFamily::c_abi_runners() -> Option<CAbiRunners>`, with `run_components`
  and `run_tiers` each their own `Option` (ThreadX will have the first and not
  the second, issue 1286). `has_c_run_components()` becomes
  `c_abi_runners().is_some()`, and `c_runner_names` is deleted, taking the
  invented ThreadX symbols with it.

## Acceptance

- A test that every key in the table maps to the same family through every
  consumer, and that an unknown key errors.
- Regression cases for the four substring victims above.

## Resolution

### The measured table, before the fix

Every key any of the four sites knew, and what each site answered. Bold marks a
wrong answer. The ZST column is the Rust pack's `board_path_for`. "csv" is
whether the proc-macro's hand-written `known_boards_csv` listed the key.

| key | `board_family` | `board_to_rtos` | Rust ZST (its family) | csv |
| --- | --- | --- | --- | --- |
| `native` | Native | posix | `LinuxBoard` (native) | yes |
| `posix` | Native | posix | `LinuxBoard` (native) | **no** |
| `zephyr` | Zephyr | zephyr | `ZephyrBoard` (zephyr) | yes |
| `fvp-aemv8r-smp` | Zephyr | **posix** | — | — |
| `armfvp` | Zephyr | **posix** | — | — |
| `nuttx` | Nuttx | nuttx | `NuttxQemu` (nuttx) | yes |
| `qemu-armv7a-nuttx` | Nuttx | nuttx | `NuttxQemu` (nuttx) | **no** |
| `rv-virt-nuttx` | Nuttx | nuttx | `NuttxQemu` (nuttx) | **no** |
| `nuttx-riscv` | **Native** (fallback) | nuttx | `NuttxQemu` (nuttx) | yes |
| `freertos` | Freertos | freertos | `Mps2An385` (freertos) | yes |
| `mps2-an385-freertos` | Freertos | freertos | `Mps2An385` (freertos) | **no** |
| `freertos-qemu-mps2-an385` | **Native** (fallback) | freertos | `Mps2An385` (freertos) | **no** |
| `freertos-posix` | Freertos | freertos | — | — |
| `s32z270-freertos` | Freertos | freertos | — | — |
| `s32z270` | Freertos | **posix** | — | — |
| `mps3-an536-freertos` | Freertos | freertos | — | — |
| `an536` | Freertos | **posix** | — | — |
| `threadx` | Threadx | threadx | — | — |
| `threadx-linux` | Threadx | threadx | `ThreadxLinux` (threadx) | yes |
| `threadx-qemu-riscv64` | Threadx | threadx | `ThreadxQemuRiscv64` (threadx) | yes |
| `rv-virt-threadx` | Threadx | threadx | `ThreadxQemuRiscv64` (threadx) | **no** |
| `esp32-qemu` | **Native** (fallback) | posix | `Esp32QemuEntry` (no RTOS) | yes |
| `esp32-c3-baremetal` | **Native** (fallback) | posix | `Esp32QemuEntry` (no RTOS) | **no** |
| `rtic-mps2-an385` | **Native** (fallback) | posix | `RticMps2An385` (no RTOS) | yes |
| `qemu-rtic-mps2-an385` | **Native** (fallback) | posix | `RticMps2An385` (no RTOS) | **no** |
| `qemu-mps2-an385` | **Native** (fallback) | posix | `Mps2An385` bare (no RTOS) | yes |
| `mps2-an385` | **Native** (fallback) | posix | `Mps2An385` bare (no RTOS) | yes |

Three things the table shows that the issue text did not:

- Two real RTOS keys were known only to the Rust pack: `nuttx-riscv` and
  `freertos-qemu-mps2-an385`. `board_family` sent them to `Native`, while
  `board_to_rtos` got them right, and only because its substring match happened
  to fit.
- `known_boards_csv`, the message printed when the macro REFUSES a key, listed
  11 of the 19 keys the macro ACCEPTS. The 8 it omitted all resolved.
- The old ThreadX arm of `c_runner_names` named `nros_board_threadx_run_tiers`,
  which is defined nowhere, and `nros_board_rtos_run_components`, which exists
  but is linked only by the FreeRTOS, NuttX and Zephyr boards (`build.rs` / board
  cmake). No ThreadX image has either symbol.

### What changed

- **One table.** It is `nros_entry_lower::BOARD_KEYS: &[(&str, BoardFamily)]`,
  21 rows: the 19 the C++ emitter knew plus `nuttx-riscv` and
  `freertos-qemu-mps2-an385`. It lives in `nros-entry-lower` because that is the
  lowest crate every consumer already depends on. Both `nros-cli-core` and
  `nros-macros` do, and the crate is serde-only, so the proc-macro can afford
  it. `known_board_keys()` iterates the table.
  - `board_family(key) -> Result<BoardFamily, UnknownBoard>` reads the table.
    `UnknownBoard`'s `Display` names every known key.
  - `board_to_rtos(key)` is now
    `board_family(key).map(BoardFamily::tier_rtos_key)`, which gives `posix` for
    the host and `as_str()` for the rest. The substring match is gone.
- **The Rust pack keeps its own key set, and a test checks it against the
  table.**
  - `nros_orchestration_ir::board_path_for` is now a lookup in
    `pub const BOARD_PATHS: &[(&str, &str)]`. The ZST paths are Rust-pack
    spelling and stay in their own crate.
  - `known_boards_csv` is derived from `BOARD_PATHS`. It is NOT derived from the
    family table, which would list keys the macro refuses (`s32z270`,
    `freertos-posix`) and drop keys it accepts (`esp32-qemu`,
    `rtic-mps2-an385`).
  - `nros-cli-core/tests/board_key_table.rs` gives each Rust ZST a family, and
    `None` for the three no-RTOS boards. It asserts that every Rust key naming
    an RTOS board is in `BOARD_KEYS` with that family, that every no-RTOS key is
    absent, and that the family-only keys are exactly the 8 C/C++ spellings.
- **An unknown key is an error at every family consumer. Each one was traced:**
  - `pack::entry_pack_for` returns `Err(message)`. `nros codegen entry-pack`
    prints it, and `NanoRosEntry.cmake` raises it as a FATAL_ERROR.
  - `cmd/codegen.rs`, the typed `nros codegen entry`, resolves the family once
    before tier resolution and before the C/C++ dispatch.
  - `emit_c::emit_typed` refuses with the known-keys message.
  - `emit_cpp::emit_typed_with_tail` refuses first. Every public entry point
    funnels through it, so the private `board_*` helpers can rely on a key that
    has already been validated.
  - `LoweredEntry::family()` returns the `Result`.
  - `plan_from_model` is the one DELIBERATE non-refuser, and the comment there
    says why. It is shared with `nros build`'s Rust entry generation, where the
    key is any Rust or out-of-tree board (`esp32-c3-baremetal`, `stm32f4` in its
    own test) and only the node list is read. Refusing there would stop
    generating those entries. So an unknown key resolves each tier from its
    platform-neutral head, via `TierDef::platform("")`, which selects no
    sub-table, rather than from `posix`'s sub-table. Every path that needs the
    family refuses the key itself.
- **In-tree callers still resolve.**
  - CMake passes `native` or `zephyr` to `entry-pack` and `codegen entry`
    (`BOARD native`/`BOARD zephyr` are the only literals, and the derived value
    is `zephyr`).
  - `_nros_node_register_entry_tu` passes the five family names.
  - `nros build` reaches `plan_from_model` only for Cargo-driven images, and the
    `native_sim/native/64` images all have a hand-written `zephyr_entry`.
- **`BoardFamily::c_abi_runners() -> Option<CAbiRunners>`**, where
  `CAbiRunners { run_components: Option<&'static str>, run_tiers: Option<&'static str> }`.
  Native, FreeRTOS, Zephyr and NuttX name the symbols defined in
  `nros-cpp/src/lib.rs` (native), `nros-board-common/c/nros_rtos_run_components.c`
  and `nros-board-{freertos,zephyr,nuttx-qemu}/c/*_run_tiers.c`. ThreadX is
  `None`, which is unchanged behaviour: a ThreadX C entry is still routed to C++.
  - `has_c_run_components()` is derived as "`run_components` is `Some`".
  - `c_runner_names` is deleted, and `emit_c` reads `c_abi_runners`. A tiered C
    entry on a family with `run_components` but no `run_tiers` is refused. That
    is the shape issue 1286 will create.
  - A test greps the board C sources and `nros-cpp` for a definition of every
    named symbol. Its negative control asserts that the scan does NOT find
    `nros_board_threadx_run_tiers`.

No golden changed: `every_emitter_matches_its_golden` and
`every_parity_case_matches_its_golden` pass on the unchanged files, because
every emitted name is the same string the deleted table produced.

### Siblings found by the sweep, not fixed here

`grep -rnE 'contains\("(freertos|zephyr|nuttx|threadx)"\)|=> "posix"' packages/cli packages/core`
finds the same substring shape in three more places. None of them reads a key
from the ENTRY namespace this table models:

- `nros-macros` `derive_target_rtos`: it takes the Rust deploy key and falls
  back to `posix`. For every in-tree Rust key it now gives the answer the table
  gives. Making it read the table would change the answer for an OUT-OF-TREE key
  whose name contains an RTOS name, and that key has no row to move to. This is
  worth doing with an explicit out-of-tree story.
- `orchestration/tier_resolver.rs` `derive_target_rtos` reads the IMAGE board,
  which is the board-catalog namespace (`native_sim/native/64` is a Zephyr
  board id there). It answers `posix` for that key, and
  `examples/workspaces/realtime-{c,cpp}` declare Zephyr tiers on such an image.
  It is latent today: the in-tree `codegen-system` invocations pass no
  `--target`. Its own doc names the right fix, which is
  `BoardDescriptor::platform` from the catalog, not this table.
- `emit_rust::emit` falls back to `LinuxBoard` for a key `board_path_for` does
  not know. Only the golden harness reaches it, because the Rust entry verb is
  retired (phase-432 W2.4).

## Follow-up (2026-09-11): the three siblings, plus two the sweep missed

Branch `fix/1285-followup-rtos-substring`. Each site now derives its RTOS from
the authority for the namespace it reads. There is ONE lookup per namespace,
and neither lookup reads the spelling of a key.

| namespace | authority | the lookup |
| --- | --- | --- |
| entry board key (`nros::main!` deploy key, `plan_from_model`) | `BOARD_KEYS` | `nros_entry_lower::tier_rtos_key_for` (lenient) or `board_family` (strict) |
| image board id (`[image.*] board`, deprecated `[deploy.*] board/kind`) | the board catalog (`packages/boards/**/nros-board.toml`) | `image::resolve_board_id` (the catalog's `resolve_deploy` rule, split out of `resolve_image_board`) then `PlatformKind::board_family` / `tier_rtos_key` |

A board with no RTOS family has ONE spelling in both namespaces:
`nros_entry_lower::NO_RTOS_TIER_KEY` (`""`). It selects no sub-table.
`resolve_tiers` refuses an authored tier on such a board with the new
`TierResolveError::NoRtosFamily`, instead of printing `[tiers.x.]`. Before
this, `plan_from_model` spelled that result `unwrap_or("")` while the macro
spelled it `"posix"`, so the two Rust producers disagreed on the same keys.

### Per site

- **`nros-macros` `derive_target_rtos`.** `Some(key)` reads
  `tier_rtos_key_for`. The worry recorded above does not arise. A key only
  reaches this function after `board_path_for` accepted it (an unknown key is
  a compile error first), so an out-of-tree key never reaches it by name. An
  out-of-tree board arrives as an explicit `board = X`, which is `None`. `None`
  keeps its documented host default (`posix`): there is no key to look up, and
  moving it would move every explicit-`LinuxBoard` entry that authors host
  tiers. What changed in-tree: the six no-RTOS Rust keys (`esp32-qemu`,
  `esp32-c3-baremetal`, `rtic-mps2-an385`, `qemu-rtic-mps2-an385`,
  `qemu-mps2-an385`, `mps2-an385`) go from `posix` to `""`. None of their
  leaves declares `[tiers]` or contracts (measured), so no image moves.
- **`tier_resolver::derive_target_rtos`** now takes a `&BoardCatalog` and
  returns `Result`. The board-id rungs (image, then deploy board, then deploy
  kind) are split into `target_board_id`.
  - No board id means the host default `posix`, and no catalog is loaded, so
    no SDK root is needed. That covers no `--target`, and a target that names
    no block.
  - An id the catalog does not know, or one several descriptors claim
    (`threadx`), is an error naming the line and the known boards.
    `codegen-system` is a verb and can refuse. A wrong sub-table is a silent
    scheduling bug, which is worse.
  - `codegen_system` loads the catalog only when a board id exists, with the
    workspace's own packages, as `nros build` does.
  - The `[deploy.*] kind` rung is the DEPLOY-kind vocabulary, not a board id.
    The first cut resolved it through the catalog and broke three
    `codegen_system` tests, because 22 in-tree `system.toml`s write
    `kind = "self"`. `self` means "this host", so it names no board and gets
    the host default. That was the old answer, and it is right for `self`.
    Any other kind (`zephyr`) resolves strictly, so a bare
    `kind = "embedded"` is refused. No in-tree block has that.
- **`native_sim/native/64` → `zephyr`, not `posix`.** It is a Zephyr board: it
  is `packages/boards/zephyr/nros-board.toml`, `platform = "zephyr"`. By the
  naming rule, `native` in it names the ROLE (a host process), not the REACH or
  the platform. Its tiers are Zephyr `k_thread`s and must read
  `[tiers.*.zephyr]`. `freertos-posix` is the same shape, and `BOARD_KEYS`
  already calls it FreeRTOS.
- **`emit_rust::emit` / `emit_lowered`** return `Result`. A key with no Rust
  ZST is an error naming the Rust pack's keys, via the new
  `nros_orchestration_ir::board_path_keys_csv`, which the macro's
  `known_boards_csv` now calls too, so the list has one spelling. It used to
  render `LinuxBoard`.
- **Two more siblings, in the TIER-KEY vocabulary** (the grep above matched
  them, and the list did not name them).
  - `rtos_realizer::sched_caps_for` had substring arms that accepted board keys
    (`threadx-linux`).
  - `derive::derive_tiers_from_contracts` had substring arms, and its `_` arm
    wrote the POSIX sub-table for anything else, including `""`.
  - Both now match the five tier keys EXACTLY. For a target with no RTOS,
    `sched_caps_for` gives the bare-metal caps, and `derive` puts no tier on
    the node and records a `tier` degradation. Every in-tree caller already
    passed a tier key.

### Keys and ids that changed answer

Every other in-tree key and id answers as before.

| site | key / id | before | after |
| --- | --- | --- | --- |
| macro | `esp32-qemu`, `esp32-c3-baremetal`, `rtic-mps2-an385`, `qemu-rtic-mps2-an385`, `qemu-mps2-an385`, `mps2-an385` | `posix` | `""` (no RTOS) |
| `codegen-system` | `native_sim/native/64`, `qemu-cortex-a53` | `posix` | `zephyr` |
| `codegen-system` | `s32z270`, `an536` | `posix` | `freertos` |
| `codegen-system` | `rtic-mps2-an385`, `qemu-mps2-an385`, `esp32-c3-baremetal`, `esp32c3` | `posix` | `""` (no RTOS) |
| `codegen-system` | `threadx` | `threadx` | error: ambiguous (as `nros build`) |
| `codegen-system` | an unknown id, e.g. `my-freertos-board` | substring (`freertos`) | error naming the known boards |
| `emit_rust` | any key without a Rust ZST, e.g. `freertos-posix` | `LinuxBoard` | error naming the Rust pack's keys |
| `sched_caps_for` / `derive` | a board key instead of a tier key | its substring | bare-metal / no tier |

### One golden changed, and why

The parity corpus case `dashed_and_rooted` named `freertos-posix`, a board
with no Rust ZST (the crate is C-only). Its golden therefore asserted a
FreeRTOS host board booting through `LinuxBoard`, which no build produces.
The case is about package names and namespaces, not the board, and the macro
side of the parity gate compares only the per-node block. So the corpus now
names `mps2-an385-freertos`, and the golden's header and `board_path` lines
say so (`::nros_board_mps2_an385_freertos::Mps2An385`). The per-node block
the gate compares is byte-identical.

### Corrected, and left open

The note above says the in-tree `codegen-system` invocations pass no
`--target`. That is wrong. `zephyr/cmake/nros_system_generate.cmake` passes
`--target zephyr-<rmw>`. That target names no image and no deploy block (the
fixtures' image is `[image.zephyr_native_sim]`; the self-pkg's deploy is
`zephyr`), so it resolves to the host default `posix` before this change and
after it. Reading `zephyr` out of `zephyr-zenoh` would be the substring match
again, so it was not done.

The bringups that module bakes today (`multi_pkg_workspace_zephyr`,
`zephyr_self_pkg`) declare no tiers, so nothing observable moves. A Zephyr
bringup WITH tiers baked through that module would read `[tiers.*.posix]`.
The fix belongs in the module: pass the image id, or a `--board`, rather than
a `<platform>-<rmw>` string. It is not filed here because issue ids are
claimed on origin.
