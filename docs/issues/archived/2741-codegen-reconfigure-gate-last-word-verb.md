---
id: 2741
title: "check-codegen-tool-reconfigure matches an emitting `nros` verb by its last word, so `profile env` must be exempt because `env` is also `cmake -E env`"
status: resolved
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

## Resolved (2026-11-23)

The gate matches the WHOLE verb phrase as the CLI spells it
(`profile env`, `ws sizing-descriptor`), as consecutive bare tokens that may
be split by any whitespace (`verb_re`), never its last word. `cmake -E env`
no longer reads as `profile env`, so `profile env` left `NOT_EMITTING` and
`NanoRosCargoProfile.cmake` is now gated rather than trusted.

Measured over the 436 tracked cmake files, old reach against new: exactly one
file differs, `cmake/NanoRosCargoProfile.cmake`, now reached by `profile env`.
Every other file reports the same verbs. Mutation: renaming that file's two
`nros_codegen_tool_reconfigure(` calls FAILS the new gate and passed the old
one.

Re-checked, and kept on purpose:
- `show` and `toolchain` are rustup's verbs. They are exempt because their
  tool is not `nros`, not because of a collision.
- `configure` is a one-word verb, so a whole-phrase match is still the bare
  token `configure`, which is cmake vocabulary (`SHAPE configure`). Its
  exemption stands on its own reason: `nros configure` keys its stamp on the
  CLI's codegen version, and its one cmake caller registers the tool.

Self-test 31/31, including `cmake -E env` (not a finding), a `profile env`
phrase split across lines (a finding), and the registered form (passes).
