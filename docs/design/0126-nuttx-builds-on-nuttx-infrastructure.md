---
rfc: 0126
title: "NuttX images build on NuttX's own build infrastructure"
status: Draft
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-550]
amends: [0120]
supersedes: []
superseded-by: null
---

# RFC-0126 -- NuttX images build on NuttX's own build infrastructure

**Status:** Draft, partly ruled (2026-11-12). The direction and eight of
the twenty questions in
[issue 2236](../issues/2236-nuttx-make-app-as-project.md) are ruled; see
"Decided" below. Q10 and Q16 are on hold, and the other ten are open (Q1 to
Q8 come from the issue's design write-up, Q9 to Q20 from its build-system
study). The work is
[phase-550](../roadmap/phase-550-nuttx-native-build-infrastructure.md), whose
items are marked by the question that gates them.

## Summary

nano-ros builds a NuttX image in three ways: the CMake leaf road, the
workspace road (`nros build`), and the `apps/external` make road. All three
restate facts NuttX already states (the Rust triple, the CPU and C flags, the
final link, how the kernel is configured, how an app is started), and the
restated copies have drifted (issue 2426). NuttX provides each of these
itself: a triple table (`apps/tools/Rust.mk`, `nuttx_add_rust`), the flags
(`ARCHCPUFLAGS`, `CROSSDEV`), an app hook (`context::`, `register::`), a
CMake app function (`nuttx_add_application`), a configure that reads
defconfig fragments (`tools/configure.sh`), and a boot entry
(`CONFIG_INIT_ENTRYPOINT`). This RFC proposes that NuttX builds take each
fact from NuttX, and that the copies we wrote are retired.

## Problem

Measured at our pins (nuttx and nuttx-apps 12.13.0; details in issue 2236's
study):

- the make road builds neither board as shipped: armv7a needs `NUTTX_DIR`,
  which it does not pass, and rv-virt compiles zenoh-pico with the host
  `cc`, because the cross tool names are an armv7a-only list;
- upstream's own Rust support does not build either board at our pins with
  our pinned nightly (no armv7a triple; a retired `panic_immediate_abort`
  feature), so "use upstream" is not free;
- the arm CPU flags are written in four places, and every copy equals
  NuttX's `ARCHCPUFLAGS` (plus `-mlittle-endian`);
- the CMake shell is unreachable (2426 item 7) and an `external/Kconfig`
  template is unused;
- RFC-0120 D5 and phase-530 Ruling R36 say the make road is "sized by its
  Kconfig knobs", but no knob symbol exists in the NuttX Kconfig: it is sized
  by crate defaults and exported `NROS_*` variables (issue 2236 section 1).

## Decided (2026-11-12)

The user's ruling, verbatim: "Accept the settled group. Hold Q10 and Q16
until 241's unified-build design". It accepts the "settled by the direction"
group of issue 2236's decision brief, except Q16, which is held with Q10.

| Q | Decision | Settles |
| --- | --- | --- |
| 9 | NuttX builds move onto NuttX's own infrastructure, retiring the inventions issue 2236's exploration lists. | D1 (direction) |
| 13 | CPU and C flags come from NuttX (`ARCHCPUFLAGS`, `CROSSDEV`), and the restated copies are deleted. | D1 (flags). Q14 (how the leaf road reads them) is open. |
| 3 | The knob channel is B1: a cargo `--config` file, with configure run in the shell's `context::`. | D2 |
| 5 | The make road drops its inline `--config` literals and `-Zbuild-std=std,panic_abort` for the board's `cargo_config`, on the guest road too. | D2 |
| 4 | When a project is named, a Kconfig RMW, edition or API flavor that differs from the image's is refused, not overridden or merged. | D3 |
| 7 | The make road counts as HOOKED in RFC-0120 D5's sense, and D5's NuttX row and phase-530 R36 are amended to match. Takes effect with a project road, so it depends on Q1 (open). | D3 |
| 17 | The board's kernel configuration becomes a defconfig fragment over upstream's `qemu-armv7a:nsh` / `rv-virt:netnsh`, configured with `tools/configure.sh`. | D7 |
| 18 | The app is named in `CONFIG_INIT_ENTRYPOINT` instead of overriding the `nsh_main` symbol, on every road. | D8 |

**On hold: Q10 (which roads exist, D4) and Q16 (NuttX's CMake flow, part of
D6).** They may conflict with the user's ruling that nano-ros has one
unified ROS-CMake build. That design, RFC-0130 (one ROS-CMake build, a draft
from the 241 session, not yet in the tree), will state how NuttX fits it as
open questions. Until then, nothing that Q10 or Q16 gates moves.

**Open:** Q1, Q2, Q6, Q8, Q11, Q12, Q14, Q15, Q19 and Q20. The brief's
recommendations for these are proposals, not decisions.

## Design (shape; ruled parts are in "Decided" above)

### D1 -- Each NuttX fact has NuttX as its owner

For every fact in the table of issue 2236's exploration (triple, CPU and C
flags, cross tool names, configure, boot entry, final link), the owner is
NuttX's configured tree. nano-ros reads it; it does not restate it. A board
descriptor keeps only what NuttX cannot state (the nano-ros board name, its
`board_config`, network evidence, the registry row). Decided: Q9 and Q13.

### D2 -- One cargo carrier on every road

The make road builds the runtime archive with the board's `cargo_config`
(build-std, target spec) and configure's knobs, through the same generated
cargo config the other roads pass with `--config` (RFC-0098 D1), not
through inline literals. Decided: Q3 (B1) and Q5.

### D3 -- The make road runs inside NuttX's `context::`

Configure (when a project is named) and the image crate generation run in the
shell's `context::` recipe, never at parse time; the cargo command carries
the kernel tree (`NUTTX_DIR=$(TOPDIR)`) and the cross names from
`$(CROSSDEV)`. Q3, Q4 and Q7 are decided. Q1 and Q2 (whether a make app
is a project, and how it names itself) are open.

### D4 -- Which roads exist

One of: both roads built from NuttX's pieces; the apps road only; or the
leaf and workspace roads only. Q10 is ON HOLD for RFC-0130.

### D5 -- The target triple and spec

The triple comes from upstream's table (with an armv7a branch from one of
Q11's sources); the rv-virt target spec lives where Q15 puts it. Measured
input: the board's JSON equals rustc's built-in
`riscv32imac-unknown-nuttx-elf` plus `eh-frame-header: false` and
`panic-strategy: abort`, both still needed where rustc links the image.

### D6 -- Every road a NuttX image can take is built by a lane

A road no lane builds is not offered: the make road gets a lane on both
armv7a and rv-virt, each in its own kernel tree (issue 0525), and NuttX's
CMake flow is offered only if it gets one too. Q16 (the CMake flow) is ON
HOLD for RFC-0130. Q19 (the cadence) is open.

### D7 -- NuttX configures its own kernel

Decided (Q17): `tools/configure.sh` over a defconfig fragment that
`#include`s upstream's board configuration, in place of `build-nuttx.sh`'s
copy and `olddefconfig`.

### D8 -- NuttX's boot entry starts the app

Decided (Q18): `CONFIG_INIT_ENTRYPOINT` names the app on every road, and the
`nsh_main` override is retired.

## Amendment to RFC-0120

D5's NuttX row and its 2026-11 changelog entry say the make road is "sized by
its Kconfig knobs". Corrected there (2026-11-11) to "sized by crate defaults
and any exported `NROS_*` variables", which is what issue 2236 section 1
read. Whether the road stays a guest road is unchanged and still open
(issue 2236 Q1 and Q7).

## Alternatives considered

- **Keep our own pieces, fix the drift in place.** Fixes 2426's items, but
  keeps four copies of the flags and a triple table nobody else maintains.
- **Delete the make road.** Removes the drift with the road; it is the road a
  NuttX user recognises, and the book offers it (Q1, Q10).
- **Adopt upstream's `RUST_CARGO_BUILD` unchanged.** Does not build at our
  pins (measured), and its environment carries no `NUTTX_DIR`, no knobs and
  no board build-std.

## Open questions

Issue 2236's "Decision brief (2026-11-12)" covers every question. Held:
Q10 and Q16, for RFC-0130. Open: Q1, Q2, Q6, Q8, Q11, Q12, Q14, Q15, Q19 and
Q20. The rest are in "Decided".

## Changelog

- 2026-11-12 -- the direction and Q3, Q4, Q5, Q7, Q9, Q13, Q17 and Q18
  ruled; Q10 and Q16 held for RFC-0130; D7 and D8 added for Q17 and Q18.

- 2026-11 -- created as a decision shape, from issue 2236's build-system
  study and exploration.
