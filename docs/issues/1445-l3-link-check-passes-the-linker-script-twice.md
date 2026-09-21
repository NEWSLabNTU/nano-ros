---
id: 1445
title: "`rust-rtos-link-check` fails on `region 'FLASH' already defined` — the
  FreeRTOS leaf's link line carries `-Tmps2_an385.ld` TWICE, so the merge queue's
  L3 lane has been red on main's own defect while the required context stayed green"
status: open
type: bug
area: [boards, tooling, ci]
severity: high
found: 2026-09-21
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
