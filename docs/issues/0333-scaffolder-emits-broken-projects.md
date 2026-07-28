---
id: 333
title: "`nros new` emits projects that don't run: the board dep becomes a TOML comment for 4 of 8 documented platforms, and the template is the retired hand-rolled entry shape"
status: open
type: bug
severity: medium
area: cli, ux
related: [rfc-0026]
---

## Finding (audit 2026-07-28, P2)

The scaffolder is the first code a new user sees. Both defects are silent.

### 1. The board dependency silently vanishes

`packages/cli/cargo-nano-ros/src/scaffold.rs:684` — the `match platform` arm for
unhandled platforms falls through to the literal string
`"# TODO: add board crate for this platform"`, which is then interpolated as the
dependency **name**:

```toml
# TODO: add board crate for this platform = { version = "*", features = [...] }
```

That whole line is a TOML **comment**, so the board dep disappears with no
diagnostic and the generated project cannot build against any board.

`book/src/reference/cli.md:90` advertises `--platform threadx|zephyr|esp32|posix`
as valid; **none of those four has an arm**. `scaffold_c`/`scaffold_cpp` go
further — `let _ = platform;` at :840 and :924 — while the same book page
promises a skeleton "tuned for the chosen platform".

Fix: validate `platform` against the board-crate table and `bail!` with the
supported list. Drive that table from `nros-sdk-index.toml` rather than a
hand-written match, so a new board cannot be advertised without being scaffoldable.

### 2. The template teaches the shape the repo abolished

`scaffold.rs:744` emits a `main.rs` that is `#![no_main]` plus
`#[unsafe(no_mangle)] extern "C" fn main() -> !` with a `loop {}` body and two
TODOs ("import your board crate", "drive your board's entry") — i.e. a project
that does not run, written in the raw-FFI style RFC-0026 and the examples
retired.

Every tracked Rust entry example is the one-liner
`nros::main!(model = "demo_bringup:config/system_model.yaml")`
(`examples/workspaces/rust/src/qemu_nuttx_entry/src/main.rs:30`,
`ws-realtime-rust/src/nuttx_entry/src/main.rs:25`, `threadx_linux_entry`,
`zephyr_entry`, …).

Fix: emit `nros::main!(...)` plus the board dep. If the macro cannot serve a
scaffolded-from-nothing project, **that gap is the finding** — file it — rather
than shipping the `no_mangle` template as the blessed shape.

## Progress — defect 1 fixed (2026-07-28)

**Defect 1 (silent board dep) is fixed.** `scaffold.rs` gained one validated
`platform_spec(platform) -> Result<PlatformSpec>` table — the single
platform → board-crate SSoT — covering all advertised platforms:

| `--platform` | board crate |
| --- | --- |
| native | (none — host build) |
| posix | nros-board-posix |
| freertos | nros-board-mps2-an385-freertos |
| baremetal | nros-board-mps2-an385 |
| nuttx | nros-board-nuttx-qemu-arm |
| threadx | nros-board-threadx-linux |
| zephyr | nros-board-zephyr |
| esp32 | nros-board-esp32-qemu |

`scaffold_package` calls it up front (fails before any file is written, like
`--rmw`/`--ros-edition`), and `scaffold_rust` reads the board crate from it, so
the `# TODO … = { … }` commented-out dep is now unrepresentable. The `esp32`
discrepancy is reconciled by adding it to the `nros new` clap `value_parser`
(`nros-cli-core/src/cmd/new.rs`), matching the crate + example + book that
already existed. The main.rs template comment now names the resolved board
crate. Verified end to end: `nros new x --platform threadx` writes
`nros-board-threadx-linux = { … }`; `--platform esp32` is accepted. Tests:
`platform_spec_covers_every_advertised_platform`,
`scaffold_rust_emits_a_real_board_dep_never_a_comment`.

The `nros-sdk-index.toml` SSoT was NOT reused: it keys boards to toolchain
package sets, carries no board-crate field, and `cargo-nano-ros` does not depend
on `nros-pkg-index`. A local const table is the pragmatic SSoT; extending the
index would be a larger change of its own.

## Still open — defect 2 (the template shape)

**Confirmed finding (the issue predicted it):** the blessed
`nros::main!(model = "…")` shape **cannot serve a scaffolded-from-nothing
project** — the macro reads the system model at *compile* time, and a fresh
scaffold has no `demo_bringup` / `config/system_model.yaml`, so it would not
build. Emitting it verbatim would trade one broken template for another.
Additionally the per-platform shape genuinely diverges (native/nuttx/threadx-linux
are hosted `std`; freertos/baremetal/esp32 are freestanding `no_std`; zephyr
builds via `west`), so a correct runnable template is per-platform, not one
string. Fixing this properly means either scaffolding a minimal bringup +
system model alongside the entry, or a macro mode that serves a modelless
project — a design task, tracked as the remaining half of this issue.
