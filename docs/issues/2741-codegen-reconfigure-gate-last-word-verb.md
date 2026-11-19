---
id: 2741
title: "check-codegen-tool-reconfigure matches an emitting `nros` verb by its last word, so `profile env` must be exempt because `env` is also `cmake -E env`"
status: open
type: tech-debt
severity: low
area: [build, cmake, gates]
related: [issue-1018, issue-2318, issue-0196]
found: 2026-11-19
---

## What happened

`scripts/check-codegen-tool-reconfigure.py` harvests the `nros` verbs that
CMake runs at configure time and requires each emitting one to register the
tool (`nros_codegen_tool_reconfigure`). It then matches call sites by the
verb's LAST WORD as a bare token (`tokens = {v.split()[-1] …}` → `VERB_RE`).

`nros dev profile env` is an emitter: its answer is the CLI's compiled cargo
profile table. Issue 2318 measured a rebuilt CLI changing that table under an
unchanged profile name, while a cached copy kept a build dir on the old
definition. Removing `profile env` from `NOT_EMITTING`, though, makes the
token `env` emitting, and the gate then flags five files that only run
`cmake -E env` (`NanoRosFeatureSet.cmake`, `nros-nuttx.cmake`,
`nros-rtos-helpers.cmake`, and two Cyclone cmake modules). So `profile env`
stays exempt. Its one real site, `NanoRosCargoProfile.cmake`, registers the
tool by hand, and nothing checks that it keeps doing so.

This is the 0196 shape: the gate's reach is narrower than its rule. Any
future emitting verb whose last word collides with a CMake or shell word
(`env`, `show`, `configure` are already exempt for the same reason) gets the
same blind spot.

## Fix direction

Match the INVOCATION, not a bare token. Either:

- match the whole verb as spelled after the resolved tool
  (`"${_nros}" dev profile env`), or
- anchor on the tool path variable: an `execute_process(COMMAND <tool var> …)`
  whose first argument is the resolved `nros` path.

Then drop the collision exemptions (`profile env`, and re-check `show`,
`toolchain` and `configure`), so that `NanoRosCargoProfile.cmake` is
gated rather than trusted.
