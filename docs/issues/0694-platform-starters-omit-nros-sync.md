---
id: 694
title: "Every platform starter's Rust build fails on a fresh clone — 12 pages, 26 build commands, zero mentions of `nros sync`"
status: open
type: bug
severity: high
area: docs, build
related: [issue-0463, issue-0457, issue-0320, phase-368]
---

## Symptom

A reader follows [FreeRTOS (QEMU)](../../book/src/getting-started/freertos.md)
top to bottom on a fresh clone. The page's Setup section is complete and
correct:

```bash
source ./activate.sh
just setup-cli
nros setup qemu-arm-freertos --rmw zenoh
```

Its Build section is the next thing they run:

```bash
cd examples/qemu-arm-freertos/rust/talker
cargo build --release
```

That command cannot succeed. Every Rust example leaf carries a
`.cargo/config.toml` whose first line is an `include` of the central,
**gitignored**, `nros sync`-generated patch table:

```toml
include = [ "../../../../../nros-patch.toml", "nros-board.toml"]
```

A fresh clone has no `nros-patch.toml`, and cargo treats a missing `include`
as a hard error during manifest parse (issue 0463 — it is not the silent drop
issues 0272 and 0457 both assumed). Reproduced verbatim:

```
error: failed to parse manifest at `<leaf>/Cargo.toml`

Caused by:
  could not load Cargo configuration

Caused by:
  failed to load config include `../../../../../nros-patch.toml` from `<leaf>/.cargo/config.toml`

Caused by:
  failed to read configuration file `<...>/nros-patch.toml`

Caused by:
  No such file or directory (os error 2)
```

Five frames, and **none of them says `nros sync`**. The reader has no way to
get from this message to the missing step. Reproduced by moving the generated
`nros-patch.toml` aside and running the page's own command in
`examples/qemu-arm-baremetal/rust/talker`; `nros sync` in that leaf regenerates
the file byte-for-byte and the build proceeds (to the next thing it needs — a
provisioned cross-compiler).

## Scope — it is the whole starter section, not one page

`nros sync` appears zero times across every platform page, against 26 build
invocations:

| page | `nros sync` | `probe=` | build commands |
|---|---|---|---|
| `freertos.md` | 0 | 0 | 4 |
| `integration-zephyr.md` | 0 | 0 | 10 |
| `zephyr.md` | 0 | 0 | 1 |
| `integration-nuttx.md` | 0 | 0 | 2 |
| `nuttx.md` | 0 | 0 | 0 |
| `threadx.md` | 0 | 0 | 2 |
| `esp32.md` | 0 | 0 | 1 |
| `integration-esp-idf.md` | 0 | 0 | 1 |
| `bare-metal.md` | 0 | 0 | 3 |
| `px4.md` | 0 | 0 | 0 |
| `integration-px4.md` | 0 | 0 | 0 |
| `arm-fvp.md` | 0 | 0 | 2 |
| `native-posix.md` | 1 | 0 | 3 |

The step is documented — but only in the Linux/Getting-Started cluster
(`first-node-rust.md`, `installation.md`, the four `workspace-*.md` pages,
`message-generation.md`). Thirteen of the book's 107 pages mention it, and
none of them is a page an embedded reader is routed to.

## Why it went unnoticed

The `probe=` column is the answer. `just probe bootstrap` executes the
tagged blocks of the book in a clean container, which is what would have
caught this — and it covers exactly three blocks on two pages
(`installation.md`, `first-node-rust.md`), both of which document the step
correctly. Every page that omits it is also every page the probe never runs.

There is a second reason nothing caught it, and it is the more useful half:
**every `just` build path syncs, and only the hand-run leaf build does not.**
`fixtures-build.sh` runs `nros sync` as a pre-pass over each row directory
(issue 0649, whose own comment says "a user runs `nros sync` once and then
builds"), and `workspace-fixtures-build.sh` does the same. So `just freertos
build-fixtures` works from a fresh clone — and the copy-pasteable
`cd <leaf> && cargo build` that the starter pages actually teach is the one
path in the repo that skips the step. CI exercises the recipes; the book
teaches the leaf build.

Contributors do not hit it either: the file is generated once per checkout,
so anyone who has ever run `just build-test-fixtures` has it, permanently,
and every starter page reads as correct forever after. This is the same
blind spot that held `pr-checks` red on main for 60+ consecutive runs before
issue 0320 — there the victim was CI's per-example `cargo fmt`, here it is
the reader.

## The rule the pages should state

The requirement is **per language, not per platform**, and the tree says so
precisely — a tracked leaf `.cargo/config.toml` exists under `rust/` leaves
and nowhere else:

```
13 qemu-arm-baremetal rust      6 qemu-riscv64-threadx rust
 6 qemu-arm-freertos  rust      6 threadx-linux        rust
 6 qemu-arm-nuttx     rust      6 zephyr               rust
 2 qemu-esp32-baremetal rust    5 workspaces           rust
```

- **Rust (cargo, and cargo under west)** — needs `nros sync` before the
  first build, on every platform.
- **C / C++ (cmake)** — does not. Those leaves have no `.cargo/config.toml`;
  codegen runs inside cmake via `nros_find_interfaces()`, and the cargo
  builds cmake drives (Corrosion → `nros-c`) resolve against the repo-root
  `.cargo/config.toml`, which carries no `include`.

So the fix is not "paste `nros sync` into 12 files". It is to state the
language-conditional rule once and route each page to it.

## A second defect in the same blocks

The same `cd <leaf> && cargo build` also writes `examples/**/target/`,
which `check-example-leaf-target-dirs` rejects — in-tree cargo builds are
expected to reach the shared group dir via `nros_fixture_target_dir_flag`
(phase-340 P2). Following `bare-metal.md` verbatim inside the checkout left
269 MB of residue and turned `just check-fast` red; observed while verifying
the fix for the first defect.

This is not the pages being wrong about cargo — a copied-out example *should*
build into its own `target/`. It is the pages teaching an in-checkout command
whose side effect the checkout forbids, without saying which situation the
reader is in. The distinction (explore in-tree with the recipe; build your own
thing from a copy) is now on the workflow page.

## Fix

Phase 368. Add the step to each starter's build path, add the
platform × language workflow table the book currently has nowhere, and
extend probe coverage so a starter's build block is executed rather than
asserted.
