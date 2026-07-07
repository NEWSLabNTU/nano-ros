---
id: 136
title: "Example naming drift — Talker vs TalkerNode, C++ namespace word order, setvbuf presence, _entry underscores, duplicate issue ids"
status: open
type: tech-debt
area: examples
related: [phase-277, phase-275, phase-242, phase-283]
---

> **Planned in [phase-283](../roadmap/phase-283-example-naming-drift-sweep.md)**
> — the mechanical sweep of items 1–3 (check→fix per wave). Item 4 (`_entry`
> rename) stays with phase-275; item 5 (duplicate ids) already resolved.

## Summary

Phase-277 unified example *content* (chatter parity W4, dep shape W6, tree
layout W7) but several small naming/style drifts remain across the tree.
None are functional bugs; they cost review time and make cross-platform
diffs noisier than they need to be. Collected here so the polish can land
as one mechanical sweep instead of ad-hoc edits.

## Drift inventory (verified 2026-07-03)

1. **Rust node struct: `Talker` vs `TalkerNode`.** The board-driven
   bare-metal examples name the struct `TalkerNode`
   (`examples/qemu-arm-baremetal/rust/{talker,talker-rtic,talker-rtic-mixed}/src/lib.rs`,
   `examples/stm32f4/rust/{talker,talker-rtic}/src/lib.rs`); every other
   platform and all workspace node pkgs use plain `Talker`. Pick one
   (plain `Talker` matches the majority and the C++ class name).

2. **C++ namespace word order differs per platform.** Same-role examples
   spell their namespace three ways:
   - `freertos_cpp_talker`, `nuttx_cpp_talker`, `threadx_linux_cpp_talker`,
     `riscv64_threadx_cpp_talker` — `<plat>_cpp_<case>`;
   - `nros_zephyr_talker_cpp` — `nros_<plat>_<case>_cpp`;
   - native C++ examples use an anonymous namespace.
   Converge on one scheme (`<plat>_cpp_<case>` is the majority).

3. **`setvbuf` presence is inconsistent.** 41 C/C++ example sources call
   `setvbuf(stdout, NULL, _IONBF/_IOLBF, …)` for unbuffered logging; some
   same-role siblings do not (e.g. `examples/zephyr/cpp/talker/` and the
   native C++ set lack it while `examples/zephyr/c/talker/src/Talker.c`
   has it). Either every hosted example needs it (test harnesses read
   line-buffered output) or none on that platform does — make it uniform
   per platform and say why in a comment.

4. **`_entry` underscore dirs.** `examples/{qemu-arm-freertos,qemu-arm-nuttx,threadx-linux}/rust/<case>_entry/`
   use a snake_case suffix while every other example dir is kebab-case.
   The rename to `-entry` is **owned by phase-275** (Entry-pkg wave);
   RFC-0026 blesses the underscore as an interim exception until that
   phase closes. Do not rename ahead of it — the fixture manifest, just
   lanes and docs all key on the current names.

5. **Duplicate issue ids (maintainer note).** `docs/issues/` carries two
   0125 files and two 0126 files
   (`0125-nuttx-rust-entry-demos-cannot-link-standalone.md` /
   `0125-rust-entry-macro-group-seed-bind-group-sched.md`,
   `0126-embedded-run-tiers-freertos-session-and-stack.md` /
   `0126-zephyr-entry-macro-no-params-tiers-lifecycle.md`). External
   references ("#125", "#126") are ambiguous. Do **not** renumber the
   files blindly — cross-references exist in roadmap docs and commit
   messages; the maintainer should decide which of each pair keeps the
   number and update the README index accordingly.

## Scope guard

- Items 1–3 are a mechanical sweep (source-only, no path changes); safe
  any time after phase-277 lands.
- Item 4 waits on phase-275; item 5 is a maintainer decision.
- `component-poc` / `component-node-poc` / `transform-poc` dir moves are
  NOT in this issue — owned by in-flight phase-242.
