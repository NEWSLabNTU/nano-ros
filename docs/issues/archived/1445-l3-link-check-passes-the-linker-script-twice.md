---
id: 1445
title: "`rust-rtos-link-check` fails on `region 'FLASH' already defined` — the
  FreeRTOS leaf's link line carries `-Tmps2_an385.ld` TWICE, so the merge queue's
  L3 lane has been red on main's own defect while the required context stayed green"
status: resolved
type: bug
area: [boards, tooling, ci]
severity: high
found: 2026-09-21
resolved_in: 2026-09-22
related: [1280, 0475]
---

## What happens

The merge queue's `queue` workflow (the L3 lane, `just ci matrix build`) fails
in `rust-rtos-link-check`:

```
rust-lld: error: …/nros-board-mps2-an385-freertos-9ff6dc5ecaf13fdd/out/mps2_an385.ld:15:
  region 'FLASH' already defined
  >>>     FLASH (rx)  : ORIGIN = 0x00000000, LENGTH = 4M
error: could not compile `freertos_rs_talker` (bin "talker")
error: recipe `rust-rtos-link-check` failed with exit code 101
```

The linker script is not malformed — it is passed **twice**. The emitted link
line ends:

```
"-Tmps2_an385.ld" "--nmagic" "--gc-sections" "-Tmps2_an385.ld" "--nmagic" "--gc-sections"
```

`-T` is cumulative, so the second copy re-reads the same `MEMORY` block and
`FLASH` is defined a second time. `--gc-sections` and `--nmagic` are duplicated
in the same pattern, which is the tell: a whole rustflags GROUP is being applied
twice, not one flag.

Measured on two consecutive merge-queue batches, both on the same frame:

| batch | run | job | conclusion |
| --- | --- | --- | --- |
| `pr-1168-a0f00adad…` | 35617631691 | 106392327093 | failure |
| `pr-1166-f5810aba4…` | 35620025319 | 106407753536 | failure |

## Why it went unnoticed

**The `queue` workflow is not the required context.** `CI` from `gate.yml` is,
and it was green for both batches — so both PRs merged while this lane was red.
A lane that cannot block is a lane whose red nobody has to read, which is issue
1040's argument one workflow over.

## What this is NOT

- **Not a PR's defect.** The two batches share nothing but their base; the flag
  duplication is in how the leaf's link is composed, which is main's.
- **Not the linker script.** `mps2_an385.ld:15` is a correct single definition;
  a second `-T` of the same file is what makes it a redefinition.
- **Not issue 0475.** That is a lib inside a raw `-Wl,` flag getting no rebuild
  EDGE. This is a flag group applied twice on one command line.

## Where the flag comes from, and the two candidate sites

`packages/boards/nros-board-mps2-an385-freertos/nros-board.toml:25` states
`"-C", "link-arg=-Tmps2_an385.ld"` — one authored copy. Under RFC-0098 D1 the
board's `cargo_config` is rendered into the leaf's generated
`build/<image>/nros-cargo.toml`, and the leaf may ALSO hold a gitignored
`.cargo/config.toml` on disk (issue 1288's carve-out). Two carriers of the same
rustflags group, both reaching one cargo invocation, would produce exactly this
line.

So the fix is one of:

1. **The generator emits the group once** — dedupe where the board's
   `cargo_config` is composed, which fixes every board at once and is the right
   place if both carriers are legitimate.
2. **The link-check road stops supplying the second carrier** — if
   `rust-rtos-link-check` adds the board rustflags on top of a config that
   already has them, the road is the bug and no board changes.

Which one is right needs the failing invocation's actual config set, which the
CI log does not print — that is the next measurement, and it is why this is
filed rather than patched.

## What would close it

`just rust-rtos-link-check` linking `freertos_rs_talker` for
`thumbv7m-none-eabi` with exactly one `-Tmps2_an385.ld` on the line, and the
merge queue's `queue` job green on a batch. A gate asserting the emitted link
line names each `-T` script once would keep it closed.

## 2026-09-22 — measured, and it is candidate 2

The measurement this filing asked for, taken on `9722fca32` against
`examples/mps2-an385-freertos/rust/talker` at its platform profile
(`nros-minsizerel`), the same leaf and profile the lane builds:

| form | result |
| --- | --- |
| `cd <leaf> && cargo build … --config build/<image>/nros-cargo.toml` (what the recipe did) | `rust-lld: error: …/mps2_an385.ld:15: region 'FLASH' already defined` |
| `cd <leaf> && cargo build …` (no `--config`) | links — a 541,724-byte `thumbv7m-none-eabi` ELF |

**The road supplied the second carrier, not the board.** `nros sync` wires the
settings file into the leaf's own (gitignored) `.cargo/config.toml` as an
`include` — issue 1381, which landed AFTER the `--config` was added to
`rust-rtos-link-check` for a different reason (the leaf's `[build] target`
moving out of `.cargo/nros-board.toml` in phase-445 W6). So the recipe named a
file cargo was already reading, cargo JOINED the two `rustflags` arrays, and
`-Tmps2_an385.ld`, `--nmagic` and `--gc-sections` each landed twice — exactly
the whole-group duplication this issue read off the link line.

The generated file had already written the rule down in its own header:

```
#   because `nros sync` wires this file into the leaf's own (gitignored)
#   `.cargo/config.toml` as an `include` — issue 1381. Do NOT combine the
#   two: cargo JOINS `rustflags` arrays across config files, so reading
#   this file twice links `link.x` twice (`region 'FLASH' already
#   defined`).
```

So candidate 1 (dedupe in the generator) is not the fix: both carriers are the
same file reached twice, not two independent renderings, and no board changes.

`leaf_cargo_config` becomes `assert_leaf_settings_included`, which keeps the
precondition the `--config` was carrying and strengthens it — the settings file
must exist AND the leaf's config must include it, since either missing means
cargo builds for the host, which is what the original flag was defending
against.

**Acceptance is still the lane.** The `queue` job has to come back green on a
batch; until it does this stays open. The remaining item this does not do is
the gate the section below asks for — one that reads the emitted link line and
asserts each `-T` script appears once. `assert_leaf_settings_included` is a
structural guard on one road, not that gate.

## Resolved — 2026-09-22, measured on the lane itself

`1a14479ec` (PR #1193) stopped the road passing the settings file a second
time. The first merge-queue batch to carry it, `pr-1193-8f0204c0`, is the
acceptance this issue asked for:

| job | run | result |
| --- | --- | --- |
| `queue` (L3, `just ci matrix build`) | 35714684742 | **success** |
| `gate` | 35714684793 | success |

and the L3 log shows the lane finishing for the first time:

```
== Phase 146.3 — embedded-RTOS Rust link check ==
  freertos talker (nros-minsizerel):
  nuttx talker (nros-minsizerel):
  threadx-linux talker (nros-relwithdebinfo):
Rust-RTOS link check OK.
```

The nuttx and threadx-linux leaves had never been REACHED in CI — freertos
failed first on every batch — so this is also the first CI evidence that those
two link at all. The batch immediately before it (`pr-1196-e15c2f1d`, run
35713216759, which predates the fix) failed on the same
`region 'FLASH' already defined`, which is what makes the change the cause and
not the weather. The batch after it (`pr-1194-1a14479e`, run 35716867158) is
green too.

**What is guarded, and what is not.** `assert_leaf_settings_included` refuses
the shape that produced this — a leaf whose settings file is missing from its
own config, and, by no longer passing `--config`, a road that reads the same
file twice. There is still **no gate that reads the emitted link line** and
asserts each `-T` script appears once, so a NEW way of duplicating a rustflags
group would once again surface only here. The other half of why that was
expensive — that the `queue` lane cannot block a merge — is issue **1447**, and
this issue's closure does not answer it.
