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
