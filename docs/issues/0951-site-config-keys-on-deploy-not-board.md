---
id: 951
title: "`[deploy.*]` is three unrelated facts in one table — site config keyed
  on the deploy name, not the board, is the half that duplicates"
status: open
type: tech-debt
area: orchestration, tooling
related: [rfc-0065, rfc-0072, phase-383, issue-0842, issue-0606]
---

## Problem

`[deploy.<name>]` accumulated three facts that have nothing to do with each
other, and each wants a different key:

| fact | example keys | what it is really about |
| --- | --- | --- |
| PLACEMENT | `kind`, `nodes` | a MACHINE |
| build description | `board`, `rmw`, `profile`, `features` | an IMAGE |
| site config | `nros.sdk`, `nros.netstack` | a BOARD |

One table, three keys, so two of the three are always keyed wrong. The
measurable consequence is duplication: **30 authored `[deploy.<n>.nros]` blocks
across 8 files held exactly THREE distinct value-sets.**

```
13 blocks  {netstack=lwip,   sdk={freertos,lwip}}       boards: mps2-an385-freertos
13 blocks  {                 sdk={nuttx,nuttx_apps}}    boards: nuttx-qemu-arm, nuttx-qemu-riscv, qemu-armv7a-nsh
 4 blocks  {netstack=netxduo,sdk={threadx,netxduo}}     boards: threadx-linux
```

The 25 duplicates exist only because the deploy key is sometimes the friendly
name (`[deploy.freertos.nros]`) and sometimes the board spelling
(`[deploy.mps2-an385-freertos.nros]`) — **two keys for one board, which is two
places for one fact to drift.** `board_facts` grew machinery to compare the
candidates and refuse a disagreement, which is detection standing in for a
shape that should not be representable.

Issue 0842 named this in its title — "site config keys on deploy target, not
board" — for a workspace with two FreeRTOS boards whose site block was
reachable from the wrong one. Its root cause turned out to lie elsewhere (the
image inherited the default `rmw`), so the keying survived the fix.

## Why the deploy key is the wrong one, concretely

A deploy name and a board name are not in bijection, in either direction:

* `[deploy.threadx-linux]` ↔ `[image.threadx]`, `[deploy.an536]` ↔
  `[image.mps3_an536]` — a mechanical `deploy.X.nros → image.X.nros` rename
  loses three site blocks in `examples/workspaces/cpp` alone;
* `examples/workspaces/rust` declares eleven `board = "native"` images, so a
  per-image site table would multiply the duplication rather than remove it;
* `examples/workspaces/rust` also carries boardless `[deploy.freertos.nros]`
  and `[deploy.nuttx.nros]` blocks whose board is only discoverable through the
  same-named image.

## Fix

`[board_config.<board>]`, a nano-ros-owned table on `SystemToml`, holding the
existing `SiteConfig` struct. The key is resolved through
`BoardCatalog::resolve_deploy` — the same rule every other board spelling goes
through (issue 0606) — so an alias finds the block exactly as it does
everywhere else, and `[board_config.freertos]` and
`[board_config."mps2-an385-freertos"]` are the same block rather than two.

Two blocks disagreeing about one board becomes **unrepresentable** instead of
detected, which is why `board_facts`'s agree/disagree comparison now has
nothing to catch for that case. It still fires for the genuinely ambiguous one:
blocks on DIFFERENT boards when the caller named neither.

## Status

Landed so far:

* **placement → `[host.<name>]`** — rlm v0.1.21 added the table; 8 machine
  blocks migrated. (The commit that landed this, `70297f148`, cites `#0939` in
  its title by mistake; 0939 is an unrelated metadata-probe bug. This issue is
  the correct number, reserved after the fact.)
* **site config → `[board_config.<board>]`** — 30 blocks became 20, plus 4 the
  generator wrote for a workspace that had none.

* **the 42 `kind = "embedded"` blocks are gone.** They were board BUILDS, which
  is what `[image.*]` means, and never placement — rlm already excluded them
  from partitioning. One (`examples/workspaces/c`'s `[deploy.an536]`) was
  dangling: it named `mps3-an536-freertos`, a board that workspace has no
  image, no entry package and no fixture row for.
* **the readers resolve `[image.*]` first** — `resolve_target` (new image
  rung), `derive_target_rtos`, `schema_build_json`, `codegen_system`'s launch
  fallback, `doctor`, `board_facts`, and `synthesise_self_bringup` (which wrote
  no images at all, silently losing every value for self-bringup packages).

* **`resolve_target` selects an IMAGE** (`--image`, alias `--target` → the sole
  image `select_default_images` picks → the sole deprecated `[deploy.<t>]`).
  `[system].default_target` is retired: it named the deploy era's concept and
  is authored nowhere. The field still parses — `SystemToml` is
  `deny_unknown_fields`, so deleting it would turn an unused key into a hard
  parse error for an out-of-tree user — and a warning says it decides nothing.
  A `default_images` naming no declared image now FAILS the plan instead of
  silently downgrading it to target-agnostic.

### A board did not record its own rustc triple

Found while making the plan derive the triple instead of copying
`[deploy.*].target`, which `[image.*]` deliberately does not carry (RFC-0065
D9 — the board descriptor owns it).

It did not own it. `BoardDescriptor::target` expects a scalar `target = "..."`
on the `[[board]]` table, and **no shipped descriptor uses that spelling**. The
triple is written inside the `cargo_config` STRING TEMPLATE — as
`[build] target = "..."`, or implied by the sole `[target.<triple>]` header —
so the field was `None` for every board in the tree.

Two consumers read it, and both failed open:

* `cmd/build.rs` drops `--target` when it is `None`, with a comment saying that
  "builds the image for the HOST — silently, since cargo is happy to", and
  claiming phase-383 W9 had fixed it. The fix never fired. Measured, by
  disabling the new inference and re-running: `nros build --dry-run freertos`
  in `examples/workspaces/rust` emits `cargo build -p freertos_entry` with **no
  `--target`**; with the inference it emits `--target thumbv7m-none-eabi`.
* `builder/preflight.rs` checks whether the board's Rust target is installed,
  so `rustup target add <triple>` was never suggested either.

Fixed by parsing the triple out of the `cargo_config` template — as TOML, not
by regex, so a triple mentioned inside a rustflag is not mistaken for a
declaration — and only when unambiguous (`[build].target`, else exactly one
`[target.*]` key). `shipped_boards_resolve_their_rustc_triple` is the
regression pin, at the layer where the fact lives.

* **`[deploy.*]` is GONE from the tree** — 0 blocks, against 96 at the start.
  The 4 misfiled fixture blocks (whose `kind` named a platform family, not a
  placement) became `[image.*]`; the 20 `kind = "self"` machines became
  unscoped `[host.<name>]` with no `nodes`, which is what a sole governing
  self-block already meant. Two templates carried `board` on the machine block
  and were split — a board is a build fact, and a machine is not a build.

  Model effect, measured across all 20 resolving workspaces: `deploy_name` +
  `kind` become `host_name`, and `target` disappears (a host says WHERE a node
  runs; the entry's `--board` decides what it is built as). Two fixtures lose
  their `execution:` section entirely, because their sole block was a platform
  family acting as placement by accident — nothing live reads per-node
  `domain`/`rmw` from a model, and `keep()`'s empty-map arm keeps every node.
  Verified the way that matters: `nros codegen entry` for
  freertos/nuttx/zephyr/native, before and after, is byte-identical for every
  board.

  The SCHEMA keeps `[deploy.*]` and every reader keeps its deploy fallback —
  out-of-tree users still author it, and the deprecation warnings are what move
  them.

Still open:
* **the last `[deploy.*]` block per bringup** — one `kind = "self"` each, the
  implicit machine the system runs on. It is a machine, so it belongs in
  `[host.*]`; moving it empties the table and gives `target = None` by
  construction, which is the shape issue 0356 wanted. It stays for now because
  it is the block that currently makes placement HAPPEN, and deleting the
  embedded blocks around it already flipped every placed node to
  `target: linux` (see below) — one measured change at a time.
* `DEPRECATED_DEPLOY_FIELDS` now records a DESTINATION per field, because the
  destination is not uniform — the first version sent `domain_id` and `locator`
  to `[image.*]`, a table with no such keys.

Found by an audit of every remaining `[deploy.*]` reader, and NOT yet fixed:

* **`resolved_domain_id` / `resolved_locator` never read `[host.*]`.** The
  deprecation lint tells users to move `domain_id` and `locator` there, and
  `SystemToml::host` then has ZERO production readers in this repo — only the
  upstream rlm placement resolver consumes it. A user who follows the lint's own
  advice gets the `[system]` default silently baked into firmware
  (`NROS_SYSTEM_DOMAIN_ID`). No in-tree fixture misbehaves today, because
  nothing authors either key outside `[system]`; it is a hole the lint opened by
  pointing at a table nobody reads. The wrinkle to solve first: `resolve_target`
  returns an IMAGE id while `[host.*]` is keyed by MACHINE name, and the link
  between them is the image's `args.host` binding — so a plain
  `self.host.get(t)` would be wrong too.
* **`nros new --deploy` still scaffolds into the retiring table.**
  `scaffold_deploy.rs` writes `board` and `target` into `[deploy.<name>]` —
  exactly the two fields the lint measured as actually firing — so the
  scaffolder emits a workspace that immediately trips its own deprecation
  warning and whose board is invisible to the image rungs. `--from-profile` can
  only fork a `[deploy.*]`, never an `[image.*]`.

Confirmed DEAD by the same audit (authored nowhere in the tree, and in the
first two cases unauthorable, since upstream's `DeployBlock` is
`deny_unknown_fields`):

* `doctor::check_deprecated_verbs` — scans `[deploy.*]` blocks for `build` /
  `package` shell-step arrays. Zero such keys exist and none could parse.
* `rtos_realizer::sched_caps_from_deploy` + `derive.rs` — read `edf` / `cores`
  from the resolved IR's deploy extras. `system.toml` cannot feed them.
  `scripts/gen-sched-matrix.py` nonetheless generates documentation asserting
  `[deploy.<board>] edf = <bool>` is a supported knob; the syntax it shows would
  be a hard parse error.

## Trap for whoever does the build half

`multi_board` in rlm is `deploy.values().any(|b| b.kind == "embedded")`, and it
forces `target = None` for every placed node. Deleting the embedded blocks
flips it to `false`, so a surviving `kind = "self"` block pins every node to
`Some(Target::Linux)`. That is benign — both `keep()` implementations compute
`board_mentioned` first, and a Linux-pinned model "mentions" only `native` and
`posix`, so an embedded board short-circuits to keeping everything, which is
the shape `examples/workspaces/features` already ships. The case to avoid is a
surviving NON-embedded block that names an MCU board: that makes
`board_mentioned` true and silently narrows every other board's entry to zero
nodes (issues 0356 / 0358 / 0320). No such block exists in-tree today.

## Verification used

Models are the artifact that would show an accident, and `nros sync` is
content-addressed — a plain sync serves a stale cache and reads as "no change".
So: `rm -rf <ws>/build/nros/models && nros sync` across all 31 workspaces,
before and after, then diff. For the site-config move the whole 456-line diff
was `sha256:` provenance lines and nothing else, which is the expected result:
site config is build environment and never reaches a SystemModel.
