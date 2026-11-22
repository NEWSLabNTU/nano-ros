---
id: 2741
title: "check-codegen-tool-reconfigure matches an emitting `nros` verb by its last word, so `profile env` must be exempt because `env` is also `cmake -E env`"
status: resolved
type: tech-debt
severity: low
area: [build, cmake, gates]
related: [issue-1018, issue-2318, issue-0196]
found: 2026-11-19
resolved: 2026-11-22
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

## Resolution (2026-11-22)

The gate matches the INVOCATION. Every harvested emitting verb except the
codegen family is matched as its whole spelling right after a tool: a quoted
path (blanked to `"   "`), a bare `${VAR}`, `nros` itself, or the head of a
`set(<var>` that builds the command for a later call (`build_verb_re`,
`_TOOL`). `codegen` and `codegen-system` keep the bare-token rule, because
their spellings in this tree are not all adjacent to the tool.

`profile env` and `configure` left `NOT_EMITTING`. `show` and `toolchain`
stay, with their own reason: the tool is rustup there, not `nros`.

Measured over the 425 tracked cmake files, old matcher against new: the new
one finds every verb the old one found, plus `profile env` in
`NanoRosCargoProfile.cmake` and `configure` in `NanoRosConfigureHook.cmake`.
Both already register the tool, so the gate stays green. The five
`cmake -E env` files are not findings. Negative control: renaming
`NanoRosCargoProfile.cmake`'s `nros_codegen_tool_reconfigure(` call makes the
new gate FAIL, naming `nros profile env`; main's gate prints OK for the same
file. Self-test cases cover `cmake -E env`, `SHAPE configure`, `profile env`
with and without the registration, and `configure` called directly and
through a `set()` variable.
