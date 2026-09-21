---
id: 1435
title: "The CLI's Rust entry template renders `<Board as BoardEntry>::run` for four board keys whose ZST does not implement `BoardEntry` — zephyr (x2) and rtic (x2) cannot compile, esp32 (x2) cannot boot"
status: resolved
type: bug
area: codegen, build
severity: low
found: 2026-09-21
resolved_in: "issue-1435 (2026-09-21)"
related: [1409, 1381, 0415, 1285, phase-432, RFC-0091]
---

## What

`packages/cli/nros-cli-core/src/codegen/entry/packs/entry/rust/entry.rs.jinja`
renders exactly one entry shape — the proc-macro's `Framework::OwnedSpin`
branch:

```rust
<{{ board_path }} as ::nros::__macro_support::nros_platform::BoardEntry>::run(…)
```

`board_path` comes from `nros_orchestration_ir::board_path_for`, which resolves
**20 keys onto 9 board ZSTs**. The template renders the same call for all 20.
Four of those keys name a ZST that does not implement `BoardEntry`, and two
more name one whose `BoardEntry::run` the RTOS never reaches:

| keys | ZST | what it implements | verdict |
| --- | --- | --- | --- |
| `zephyr`, `native_sim/native/64` | `nros_board_zephyr::ZephyrBoard` | `BoardInit`, `BoardPrint`, `BoardExit` + inherent `wait_link_up` | **does not compile** |
| `rtic-mps2-an385`, `qemu-rtic-mps2-an385` | `nros_board_mps2_an385::RticMps2An385` | `BoardInit`, `BoardPrint`, `BoardExit`, `RticBoardEntry` | **does not compile** |
| `esp32-qemu`, `esp32-c3-baremetal` | `nros_board_esp32_qemu::Esp32QemuEntry` | `BoardEntry` | compiles, **does not boot** |

The measurement for the first row, on a tree where 1409 is not yet applied so
both defects appear at once — the `#![no_std]` shell `builder/entry.rs` writes
for `EntryKind::ZephyrStaticlib`, plus this template's `zephyr` rendering,
compiled against stub crates that carry the board's REAL impl set:

```
error[E0433]: cannot find `std` in the crate root          <- issue 1409
  --> entry_zephyr.rs:38:11   ::std::eprintln!(…)
error[E0433]: cannot find `std` in the crate root          <- issue 1409
  --> entry_zephyr.rs:39:11   ::std::process::exit(1);
error[E0277]: the trait bound `ZephyrBoard: BoardEntry` is not satisfied
  --> entry_zephyr.rs:17:6                                 <- THIS ISSUE
   | <::nros_board_zephyr::ZephyrBoard as …::BoardEntry>::run(
   |  ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the trait `BoardEntry` is not
   |                                    implemented for `ZephyrBoard`
```

and the same three for `rtic-mps2-an385` against `RticMps2An385`. The two
`E0433`s are 1409's and go away with its fix; the `E0277` does not, which is
what makes this a separate issue rather than 1409 fallout. 1409's own commit
message names it and leaves it alone.

The ZST's impl set is measured, not inferred:

```
$ grep -rn "BoardEntry" packages/boards/nros-board-zephyr/src/
src/lib.rs:7://! Zephyr, so the usual `<Board as BoardEntry>::run(setup)` shape
src/lib.rs:45://! `BoardEntry` story (e.g. Phase 212.N.7 once the legacy zephyr-rust
$ grep -rn "for RticMps2An385" packages/boards/nros-board-mps2-an385/src/
src/rtic.rs:138:impl BoardInit for RticMps2An385 {
src/rtic.rs:142:impl BoardPrint for RticMps2An385 {
src/rtic.rs:148:impl BoardExit for RticMps2An385 {
src/rtic.rs:311:impl RticBoardEntry for RticMps2An385 {
```

Two mentions in doc comments and zero impls for Zephyr; `RticBoardEntry` is a
separate trait (`pub trait RticBoardEntry: Board`), not a subtrait of
`BoardEntry`. And there is no blanket impl to rescue either: the one the book
once described, `impl<B: Board + TransportBringup + NetworkWait> BoardEntry for
B`, was removed in phase-206 W4 (issue 1067) precisely because it OVERLAPS the
twelve direct impls.

`NetworkWait` no longer exists as a trait, so "the Zephyr board implements
`NetworkWait`" — the usual way this is described — is stale by one phase;
`wait_link_up` survives as an INHERENT method.

## Does it bite a user today? No — and the reason is narrow

Same three measurements 1409 recorded, re-checked here:

1. `nros codegen entry --lang rust` is **retired** (phase-432 W2.4).
   `run_entry`'s emit `match lang` has one `Lang::Rust` arm and it is an
   unconditional `bail!`.
2. `builder/entry.rs` writes its `#![no_std]` shell over a `nros::main!(…)`
   body, never over this rendering, on purpose.
3. `git grep 'emit_rust::' -- packages/` returns three lines. One is the dead
   `golden.rs` dispatch; the other two are test harnesses (`golden.rs`,
   `parity.rs`) that consume the output as TEXT and never compile it.

So nothing renders a Zephyr or RTIC Rust entry through this template. What is
broken is the ARTIFACT, and the artifact is what the goldens and the
proc-macro-parity gate exist to hold still.

## Why no gate caught it

Three near misses, each narrower than the rule it enforces:

- **The board-key refusal** (`emit_lowered`, issue 1285 follow-up) rejects a key
  with no Rust board ZST. `zephyr` HAS one, so a known key with an unusable ZST
  walks straight through. This is the hole 1409's work predicted.
- **The goldens** cover `native` (`rust_native_one`, `rust_native_rich`) and the
  parity corpus covers `native` + `mps2-an385-freertos`. All three are
  `owned-spin` boards. **No golden renders a Zephyr, RTIC or ESP32 Rust entry**,
  so nothing pinned the broken bytes.
- **`nros-macros`' `in_tree_board_keys_resolve_to_an_emit_shape`** iterates a
  HAND-WRITTEN list of ten keys, not `BOARD_PATHS`. That is why the second hole
  below went unseen.

## The second hole: `native_sim/native/64` has no framework row

`nros_orchestration_ir::framework_for_board_key` is the SSoT for "which entry
shape does this board want". It maps `zephyr` → `"zephyr"`, the two RTIC keys →
`"rtic"` and the two ESP32 keys → `"esp32"`. It does **not** map
`native_sim/native/64`, which phase-445 W5 added to `BOARD_PATHS` as the
zephyr board's second name.

That key therefore resolves to `None`, which every caller reads as
`owned-spin` — so **`nros::main!` itself** would emit
`<ZephyrBoard as BoardEntry>::run` for `deploy = "native_sim/native/64"`, the
same `E0277`. It is latent for the same kind of reason as the template:

- no board crate declares `[package.metadata.nros.board] framework` (measured:
  `git grep 'framework = ' -- packages/boards/*/Cargo.toml` finds one COMMENT
  and no value), so `NROS_BOARD_FRAMEWORK` is never set in-tree and the table
  is the only route;
- every live Rust Zephyr example writes `board = "zephyr"`
  (`examples/zephyr/rust/*/system.toml`, six of six);
- the three in-tree `system.toml`s that DO write `native_sim/native/64` are
  fixtures whose entry is hand-written C/CMake (`entry = "zephyr_app"`) or a
  bare `fn main() {}`.

So no live build reaches it — but a Rust workspace entry on a Zephyr image
spelled that way would, and the diagnostic it would get is about a trait bound
rather than about the board.

## Fix

Three options were on the table. The shape of the problem decides between
them: this emitter renders ONE framework's shape and accepts every board key
regardless of framework, while its own doc comment already says "RTIC and
Embassy stay proc-macro-only".

### Rejected — make the template's Zephyr arm match the macro

The macro's `Framework::Zephyr` arm is not a different spelling of
`BoardEntry::run`; it is a different ENTRY, and it consumes facts this producer
does not have. Its body splices `#zephyr_rmw_register_ts` (the backend register,
from the entry crate's own `rmw-<x>` feature) and `#zephyr_body_tail` (the
spin-or-tiers branch, from the resolved tiers), plus the locator override hook,
the `nros_platform::log::init_default()` sink list and the `wait_network` gate.
`LoweredEntry` — the type the parity corpus is made of, and the whole of what
this renderer receives — carries `bringup`, `launch`, `board`, `depfiles` and
`nodes`. Nothing else.

So a matching arm means extending `LoweredEntry` and both renderers, and a
PARTIAL arm is worse than the bug: without the backend register the CFFI
registry is empty and `Executor::open` fails `Transport(ConnectionFailed)` —
the macro's own comment says so. That trades a compile error for a runtime one,
which is the direction this project does not move in. The same argument
disposes of RTIC and Embassy for the reason `emit`'s doc comment already gives
(`proc_macro::Span` for the `custom_tasks` splice) and of ESP32 for the reason
`Framework::Esp32`'s does (`#[::esp_hal::main]`).

### Rejected — delete the emitter

The verb is retired, so the temptation is real. But the emitter is not dead
code: it is one of the two things the phase-432 W2.4 parity gate compares.
`nros-macros`' `entry_parity::the_two_rust_entry_producers_render_the_corpus_identically`
renders the shared corpus with `quote!` and diffs it against this renderer's
goldens, which is the byte-diff `emit_rust.rs` had claimed since 2024 and did
not have until W2.4 built it — and building it immediately found a real
divergence (`1, 2, 5` vs `quote!`'s `Literal::u8_suffixed` `1u8, 2u8, 5u32`).
Deleting the emitter deletes that gate and leaves the proc-macro as the only
Rust entry producer with nothing checking what it emits. RFC-0091 §7 is the
decision that says so; this issue is not the place to reverse it.

### Chosen — refuse, in the emitter, every board whose framework is not `owned-spin`

The smallest change that makes the artifact honest, and it reuses a mechanism
that already exists: `emit_lowered` already refuses a board key with no Rust
ZST (issue 1285 follow-up), with a test. This adds a second refusal beside it,
predicated on `nros_orchestration_ir::framework_for_board_key` — the SSoT both
Rust producers already consult, so a new non-owned-spin board is refused the
day its framework row is written, with no edit to the emitter.

What it costs: nothing the goldens force. No golden and no parity case renders
a non-owned-spin board (`native`, `native`, `mps2-an385-freertos`), so no
committed bytes move and the parity gate is untouched. What it buys: the
emitter's doc comment becomes true, and a future reader who reaches for this
renderer for a Zephyr entry gets a sentence naming `nros::main!` instead of an
`E0277` about a trait bound.

ESP32 is refused with the other two although its ZST DOES implement
`BoardEntry`, because the rule is "this emitter renders one framework", not
"this emitter renders whatever type-checks". Its rendering compiles and cannot
boot, which is the quieter of the two failures and therefore the worse one to
leave renderable.

## Resolution

`status: resolved`. Two halves.

**1. The emitter refuses.** `emit_rust::refuse_non_owned_spin` runs in
`emit_lowered` right after the board-ZST lookup. Six of the twenty keys are
refused — `esp32-qemu`, `esp32-c3-baremetal`, `zephyr`,
`native_sim/native/64`, `rtic-mps2-an385`, `qemu-rtic-mps2-an385` — each with a
message naming the board, the shape it wanted and the producer that has it:

```
the Rust entry pack renders the `owned-spin` entry shape
(`<Board as BoardEntry>::run`), and board `zephyr` wants the `zephyr` shape.
Use the `nros::main!()` proc-macro, which is the canonical Rust entry emitter
and has a branch for it.
```

**2. `native_sim/native/64` gains its framework row.** It was in `BOARD_PATHS`
and not in `framework_for_board_key`, so a framework-keyed refusal would have
MISSED it — and the same hole made `nros::main!` itself emit `owned-spin` for
that key. Both names of the zephyr board descriptor now map to `"zephyr"`.

### Acceptance — the defect, compiled

`tmp/1435/`. The two halves the tools themselves write — `builder/entry.rs`'s
`#![no_std]` shell for `EntryKind::ZephyrStaticlib` (and
`#![no_std]`/`#![no_main]` for `BoardRun`), concatenated with this template's
rendering — compiled for the host against stub crates whose board ZSTs carry
the REAL impl sets (`ZephyrBoard`: `BoardInit`/`BoardPrint`/`BoardExit` +
inherent `wait_link_up`; `RticMps2An385`: those three + `RticBoardEntry`):

```
$ rustc --edition 2024 --crate-type rlib … entry_zephyr.rs --emit=metadata ; echo rc=$?
error[E0433]: cannot find `std` in the crate root       (issue 1409, x2)
error[E0277]: the trait bound `ZephyrBoard: BoardEntry` is not satisfied
  --> entry_zephyr.rs:17:6
error: aborting due to 3 previous errors
rc=1

$ rustc … entry_rtic.rs --emit=metadata ; echo rc=$?
error[E0433]: cannot find `std` in the crate root       (issue 1409, x2)
error[E0277]: the trait bound `RticMps2An385: BoardEntry` is not satisfied
  --> entry_rtic.rs:18:6
error: aborting due to 3 previous errors
rc=1
```

After the fix there is nothing to compile: both keys are refused before a byte
is rendered. That is the acceptance for a refusal — the artifact that could not
compile no longer exists.

### Tests

- `emit_rust::tests::a_board_wanting_another_framework_is_refused_not_rendered_as_owned_spin`
  — per KEY over the whole table, keyed on the framework SSoT rather than on a
  list of board names (a list is what `nros-macros`'
  `in_tree_board_keys_resolve_to_an_emit_shape` already is: it names ten of the
  twenty keys, and the one that was wrong is not among them). Checks the
  rendering side too — an accepted key must really render `<Zst as
  BoardEntry>::run` — and pins the refused set.
- `emit_rust::tests::esp32_is_refused_although_its_zst_does_implement_board_entry`
  — the one a reader will argue with, spelled out.
- `nros_orchestration_ir::tests::every_key_of_one_board_zst_wants_one_framework`
  — the second half, as an invariant rather than a row: two keys naming the same
  board ZST must want the same entry shape.

Mutation-tested, both halves:

- dropping the `native_sim/native/64` arm →
  ``` `::nros_board_zephyr::ZephyrBoard` is named by `zephyr` (framework
  `zephyr`) and by `native_sim/native/64` (framework `owned-spin`) ``` from the
  IR test, and a refused-set mismatch from the CLI test. Both, independently.
- turning the refusal into `let _ = refuse_non_owned_spin(…)` →
  ``` `esp32-qemu` wants the `esp32` entry shape and was RENDERED anyway ```.

### Not done here

`in_tree_board_keys_resolve_to_an_emit_shape` in `nros-macros` still iterates a
hand-written key list instead of `BOARD_PATHS`. Widening it is a
`packages/core/nros-macros/**` edit, which issue 1410 owns concurrently; the IR
test added here covers the same hole from the table side in the meantime.
