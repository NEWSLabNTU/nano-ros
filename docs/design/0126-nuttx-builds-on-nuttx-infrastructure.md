---
rfc: 0126
title: "NuttX images build on NuttX's own build infrastructure"
status: Draft
area: platform-board-rtos
since: 2026-11
last-reviewed: 2026-11
implements-tracked-by: [phase-550]
amends: [0120]
supersedes: []
superseded-by: null
---

# RFC-0126 -- NuttX images build on NuttX's own build infrastructure

**Status:** Draft; every decision ruled (2026-11-12), implementation not yet
done. All twenty questions in
[issue 2236](../issues/2236-nuttx-make-app-as-project.md) are ruled, in two
rulings recorded in "Decided" below (Q1 to Q8 come from the issue's design
write-up, Q9 to Q20 from its build-system study). The work is
[phase-550](../roadmap/phase-550-nuttx-native-build-infrastructure.md).

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

The user ruled in two steps, both on 2026-11-12 and relayed through the
manager session.

1. On the decision brief's "settled by the direction" group, verbatim:
   "Accept the settled group. Hold Q10 and Q16 until 241's unified-build
   design".
2. On the rest, after a review that preferred "better UX and
   maintainability, and design cleanliness": "Accept". This covered the
   list below, including Q10 and Q16, which the unified-build design
   (RFC-0130, one ROS-CMake build) now places.

| Q | Decision | Where |
| --- | --- | --- |
| 9 | NuttX builds move onto NuttX's own infrastructure, retiring the inventions issue 2236's exploration lists. | D1 |
| 13 | CPU and C flags come from NuttX (`ARCHCPUFLAGS`, `CROSSDEV`), and the restated copies are deleted. | D1 |
| 14 | `nros configure` reads the kernel export's `scripts/Make.defs`, which is a provisioned input, not a host-build product of the image being configured. RFC-0120 D1 gains the sentence below. | D1 |
| 3 | The knob channel is B1: a cargo `--config` file, with configure run in the shell's `context::`. | D2 |
| 5 | The make road drops its inline `--config` literals and `-Zbuild-std=std,panic_abort` for the board's `cargo_config`, on the guest road too. | D2 |
| 12 | (b) Our cargo command, carrying the facts from NuttX. Upstream's `RUST_CARGO_BUILD` is not used. | D2 |
| 1 | Yes. A make app may NAME a project, for its configuration and its sizing. | D3 |
| 2 | **A2.** The app declares itself in its own `Make.defs`: `NROS_PROJECTS += $(APPDIR)/external/<app>`, with an optional `NROS_IMAGE := <id>`. No path goes in a defconfig. A guest (no `NROS_PROJECTS`) is unchanged by construction. More than one project is refused, naming them. | D3 |
| 4 | When a project is named, a Kconfig RMW, edition or API flavor that differs from the image's is refused, not overridden or merged. | D3 |
| 6 | Yes. A make-app project declares its entities (`entities = [...]` or `"defaults"`). A make census is a separate issue. | D3 |
| 7 | The make road counts as HOOKED in RFC-0120 D5's sense when it names a project, and D5's NuttX row and phase-530 R36 are amended to match. | D3 |
| 8 | No. The guest road gets no Kconfig knob prompts: that would be a second carrier beside Q3's file. | D2 |
| 10 | **One road for nano-ros packages:** the unified ROS-CMake road (RFC-0130 D7), building against NuttX's exported make tree. The leaf and workspace roads become that one road. The `apps/external` make app is a GUEST road (nano-ros entering a NuttX user's own tree), not a second way to build a nano-ros package. | D4 |
| 11 | **(b)** One armv7a fallback, in ONE place, checked against upstream's triple table for every triple upstream knows, plus a contribution of the armv7a branch upstream. No nuttx-apps fork. The fallback is deleted when the pin gains the branch. | D5 |
| 15 | **(a)** The board's rv-virt JSON target spec on EVERY road: one statement. Upstreaming it to rustc or to NuttX stays possible later. | D5 |
| 16 | **(b)** Delete NuttX's CMake shell (`integrations/nuttx/CMakeLists.txt`) and the book's sentence. Per RFC-0130 Q-N1, the exported make tree is the road now; NuttX's upstream CMake flow comes only after it is measured booting both boards and a lane builds it. | D6 |
| 19 | The make-road lane (armv7a and rv-virt, each in its own scratch kernel tree) runs daily. | D6 |
| 17 | The board's kernel configuration becomes a defconfig fragment over upstream's `qemu-armv7a:nsh` / `rv-virt:netnsh`, configured with `tools/configure.sh`. | D7 |
| 18 | The app is named in `CONFIG_INIT_ENTRYPOINT` instead of overriding the `nsh_main` symbol, on every road. | D8 |
| 20 | Retiring the vendored `libc` fork is a separate phase, opened once no road builds `std`. | -- |

## Design

### D1 -- Each NuttX fact has NuttX as its owner

For every fact in the table of issue 2236's exploration (triple, CPU and C
flags, cross tool names, configure, boot entry, final link), the owner is
NuttX's configured tree. nano-ros reads it; it does not restate it. A board
descriptor keeps only what NuttX cannot state (the nano-ros board name, its
`board_config`, network evidence, the registry row). Decided: Q9 and Q13.
On the exported-tree road the flags are read from the export's
`scripts/Make.defs` (Q14).

### D2 -- One cargo carrier on every road

The make road builds the runtime archive with the board's `cargo_config`
(build-std, target spec) and configure's knobs, through the same generated
cargo config the other roads pass with `--config` (RFC-0098 D1), not
through inline literals, and the cargo command is ours (Q12 b). Decided:
Q3 (B1), Q5, Q8 (no second carrier), Q12.

### D3 -- The make road runs inside NuttX's `context::`

Configure (when a project is named) and the image crate generation run in the
shell's `context::` recipe, never at parse time; the cargo command carries
the kernel tree (`NUTTX_DIR=$(TOPDIR)`) and the cross names from
`$(CROSSDEV)`. Decided: Q1 (a make app may name a project), Q2 (A2, the
app's own `Make.defs`), Q4, Q6 (declared entities) and Q7.

### D4 -- Which roads exist

Decided (Q10): one road builds nano-ros packages, the unified ROS-CMake road
of RFC-0130 D7, building against NuttX's exported make tree. Today's leaf and
workspace roads become that one road. The `apps/external` make app stays as a
GUEST road, for nano-ros entering a NuttX user's own tree; it is not a second
way to build a nano-ros package.

### D5 -- The target triple and spec

Decided (Q11 b): the triple comes from upstream's table, with ONE armv7a
fallback in one place, checked against upstream's table for every triple
upstream knows. The armv7a branch is contributed upstream, and the fallback
is deleted when the pin gains it. No nuttx-apps fork. Decided (Q15 a): the
board's rv-virt JSON target spec is used on every road. Measured
input: the board's JSON equals rustc's built-in
`riscv32imac-unknown-nuttx-elf` plus `eh-frame-header: false` and
`panic-strategy: abort`, both still needed where rustc links the image.

### D6 -- Every road a NuttX image can take is built by a lane

A road no lane builds is not offered: the make road gets a lane on both
armv7a and rv-virt, each in its own kernel tree (issue 0525), and NuttX's
CMake flow is not offered: its shell is deleted (Q16 b) until it is measured
booting both boards and a lane builds it (RFC-0130 Q-N1). The make-road lane
runs daily (Q19).

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
read. Q1 and Q7 now rule that a make app naming a project is hooked; a
make app that names none stays a guest road.

Q14 adds one sentence to RFC-0120 D1: a NuttX kernel export is a provisioned
input, built once per arch and shared by every image, so layer 0 may read its
`scripts/Make.defs`; it is not a host-build product of the image being
configured.

## Alternatives considered

- **Keep our own pieces, fix the drift in place.** Fixes 2426's items, but
  keeps four copies of the flags and a triple table nobody else maintains.
- **Delete the make road.** Removes the drift with the road; it is the road a
  NuttX user recognises, and the book offers it (Q1, Q10).
- **Adopt upstream's `RUST_CARGO_BUILD` unchanged.** Does not build at our
  pins (measured), and its environment carries no `NUTTX_DIR`, no knobs and
  no board build-std.

## Open questions

None: every question in issue 2236 is ruled (see "Decided").

## Changelog

- 2026-11-12 -- all twenty questions ruled, in two steps: the settled group
  first (with Q10 and Q16 briefly held for RFC-0130), then the rest. D4 is
  one road (RFC-0130 D7) plus the make app as a guest road; D7 and D8 added
  for Q17 and Q18.

- 2026-11 -- created as a decision shape, from issue 2236's build-system
  study and exploration.
