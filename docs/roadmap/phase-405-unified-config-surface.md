# Phase 405 — one configuration surface: name the SSoT, project the rest, delete the strays

**Status (2026-08-31). Design + the enumerator landed; no cleanup executed.**
Opened from [issue 0934](../issues/0934-config-redundancy-map.md)'s survey and
[issue 0931](../issues/0931-retire-model-and-default-launch.md)'s argument-surface
count. `book/src/reference/configuration-surface.md` and its gate are in; every
wave below is unstarted.

## The problem, stated as a number

A user asking "where do I set the RMW?" has **eight** answers. The receive
locator has **nine**. Domain id has seven, ROS edition six, board six. Those are
not eight choices — most are the mechanism by which one decision crosses a layer
boundary — but nothing anywhere recorded which was which, so a reader cannot
tell a decision from its plumbing.

## The layer model, which took three wrong rules to find

The classification rule in `scripts/gen-config-surface.py` was wrong three
times, and each version would have deleted a working knob. Recording that here
because the same mistakes are available to the next person:

1. **Wrong layer.** Splitting symbols into "public" and "carrier" by asking who
   READS them called 24 of them carriers because only cmake did. But cmake
   reading `${CONFIG_X}` to emit `ZPICO_X=<value>` is cmake CARRYING a choice,
   not owning it. Kconfig already answers the question — a symbol with a PROMPT
   is one the user is asked about, and all 79 have one.
2. **Missed Kconfig itself.** A tree-wide grep that skips Kconfig cannot see
   `select` / `depends on`. `NROS_TRANSPORT_SERIAL` has its entire effect by
   `select NROS_ZENOH_LINK_SERIAL`; nothing reads it BY DESIGN.
3. **Missed user code.** `NROS_INIT_DELAY_MS` has no reader in this tree because
   its consumer is an application's own `main()` — the guides teach exactly that.
   Generating the macro is the service.

**So the model is:**

| layer | examples | who sets it |
| --- | --- | --- |
| **authored config** | `system.toml`, launch files, `nros-codegen.toml`, Kconfig symbols | the USER |
| **carriers** | `ZPICO_*` / `NROS_*` env, `-D` cache vars, compile definitions | the BUILD, never a human |

Setting a carrier by hand bypasses the question the authored layer asked. That
is the sentence the surface page now leads with, and the rule this phase
enforces.

## The invariant

**For every fact: one SSoT; every other site is a PROJECTION with a gate, or it
is deleted.** Not "remove duplicates" — several duplicates are load-bearing
transport and must stay. What must not stay is an unstated second AUTHORITY.

## Waves

**W1 — the strays.** Findings that are defects on their own, independent of the
SSoT work, and cheap:

* `nros new-entry` writes `CONFIG_NROS_XRCE_AGENT_LOCATOR` into every XRCE
  scaffold; **no such symbol exists** (the real pair is `..._AGENT_ADDR` +
  `_PORT`). Zephyr discards unknown symbols silently, so the line is inert.
* `nros_feature_set(BOARD …)` — its own comment says "accepted but UNUSED since
  phase-338 W5.a".
* `nano_ros_entry`'s `ARGS` / `LAUNCH_ARGS` / `MODEL` / `LOCATOR` — zero
  authored users each (issue 0931). `LAUNCH_ARGS` has no path at all through the
  `nano_ros_add_executable` verb.
* `nano_ros_add_executable` accepts `BRINGUP`/`PANIC` **by accident**: it does
  not parse them, so they land in `UNPARSED_ARGUMENTS`, are spliced into
  `_srcs`, forwarded as `SOURCES`, and re-tokenized back into keywords by
  `nano_ros_entry`. Three in-tree entries depend on that. Add one positional
  source to any of them and it breaks.

**W2 — `rmw` and `board`, the two that can disagree silently.** These have real
consequences and go first among the SSoT work. The bringup's resolved model is
already where both end up; `package.xml`'s `<nano_ros rmw= board=>` tuple and
the cmake cache var are a SEPARATE resolution path that never meets
`resolved_rmw`. Make them read the model, or gate them against it.

Concrete and shipped today: `examples/templates/pure-c-workspace/CMakeLists.txt`
says `BACKEND zenoh` on one line and names a bringup whose `system.toml` says
`rmw = "zenoh"` on the next. Two authored copies in one call, and nothing
compares them — `NanoRosWorkspace.cmake` builds the path to that file and never
parses it. **Start here: a gate that compares them is a day's work and catches
the class.**

**W3 — `domain_id`, `locator`, `ros_edition`.** Same treatment, lower stakes.
Note `ros_edition` has THREE independent defaulting sites that all say `humble`;
they agree by coincidence, not derivation.

**W4 — `nano_ros_entry` from eleven arguments to five** (issue 0931). `LAUNCH`
drops entirely (all nine authored uses say `default`); `MODEL`/`LOCATOR`/`ARGS`/
`LAUNCH_ARGS` go with W1; `LANG` is already inferred from `SOURCES`;
`DEPLOY`/`BOARD` become derivable.

**The ordering constraint that would break a naive sweep:** cmake asks for
`DEPLOY`/`BOARD` because its gate at `NanoRosEntry.cmake:159` runs BEFORE the
model is resolved around line 270. Deleting the arguments without moving the
gate fails every non-native entry.

**W5 — execute the deprecations already declared.** RFC-0065 D6 opened a window
on `[deploy.*]`'s build fields; `[system].features` supersedes the typed
`[safety]`/`[param_services]`/`[lifecycle]` blocks. Both windows are open and
neither has been closed. `examples/workspaces/features` still uses BOTH
capability spellings in one file.

**Executed 2026-08-31, and the second half is BLOCKED — on code, not on data.**

*Capabilities (R11).* The window is already closed in the data: **zero** in-tree
`system.toml` carries `[safety]` or `[param_services]`, which are the only two
`deprecated_typed_capability_blocks()` names. The one typed block left is
`examples/workspaces/features`'s `[lifecycle]`, and it is **not** the deprecated
spelling and **not** convertible — `features = ["lifecycle"]` can only say ON,
while `autostart = "active"` is read from the typed block alone
(`planner.rs:636`, keyed on `s.lifecycle.is_some()`, not on `capability_enabled`).
So the double declaration was resolved the only non-lossy way: the redundant
`"lifecycle"` was dropped from `[system].features` and the block kept. Effective
set unchanged (`nros config show --format cmake` still emits
`param_services;lifecycle`); only the provenance line moves from
`[<axis>] + [system] features` to `[<axis>]`.

*`[deploy.*]` build fields (R12).* **Nothing is safely removable today.**
`profile` and `features` have zero in-tree uses; `rmw` has two, both in
`multi_pkg_workspace_{freertos,nuttx}` fixtures that declare no `[image.*]` at
all, so no image supplies the value. That leaves `board` — and **`board` is not
only a build field, it is the JOIN KEY for site config.** `nros ws board-facts`
resolves `[deploy.<t>.nros]` (SDK roots, netstack, `NROS_BOARD_TOML`) by matching
`--board` against `t.board`, or by `--deploy` and then requiring `t.board`
(`board_facts.rs:112`, `:274`); it has **no `[image.*]` fallback**. Measured on
the four blocks that pass the "an image already supplies the same value, and the
deploy carries no `.nros`" test — `c`/`cpp` `freertos-posix`, `cpp` `an536`,
`rust` `esp32-qemu` — removing `board` turns a working resolution into
`no [deploy.*] with board = "…"` / `names no board`, and
`nros_resolve_board_facts` treats that as SOFT, so the build loses `NROS_BOARD` /
`NROS_BOARD_TOML` / `NROS_NETSTACK` **silently**. `check-site-config.py` keys on
the same field (`if board not in BOARDS: continue`), so it would stop checking
the site block rather than fail.

Corroborating: `examples/workspaces/mixed`'s `[deploy.freertos.nros]` is ALREADY
unreachable for exactly this reason — that block names no `board`, so
`board-facts --deploy freertos` errors instead of delivering it. The hazard is
live, not hypothetical.

**So R12 needs a code wave before a data wave:** teach `board-facts` (and the two
gates) to resolve a board through `[image.*]`, then delete the `[deploy.*].board`
copies. That belongs with W2 — it is the same "make the other site a projection
of the model" work — and D6's *"become deletable once their `[image.*]` lands"*
should be read as "once the RESOLVER reads the image", not "once the image is
written".

**W6 — the gated duplications.** SDK roots exist four times and the zenoh tx
trio three; both are asserted-equal by a gate rather than merged.
`scripts/check-zephyr-knob-agreement.py` states the case against itself better
than this doc can: *"Two spellings of one fact is the drift this repo keeps
paying for … **Not a substitute for merging the sources.**"* Decide per pair
whether the gate IS the end state — for SDK roots it may be, since the TOML
indirects to the env rather than restating it.

## Not settled — do not act without more evidence

* `platform` appears in `nros-sdk-index.toml`, `nros-board.toml` and
  `board-support.toml`, but the KEY SPACES differ, so these may be three
  keyings of a board that each carry a platform column rather than one fact
  three times.
* Whether `[deploy.*].{framework,optimize,target}` have any nano-ros reader.
* Whether the Kconfig 0-31 priority band and `[tiers.*]` raw priorities are one
  fact or two — the Kconfig help says the mapping is image-dependent.
* Whether the 17 C-lane-only zenoh Kconfig symbols need a Rust reader. Answering
  needs a build, not a grep.

## How this phase avoids the mistake it was created by

A generated verdict is not evidence. The dead bucket this campaign set out to
empty was empty already — three times a rule reported a live knob as dead, and
only reading the symbol caught it. **Before deleting anything in W1, read the
declaration and find its effect**, including `select`, application code, and
documentation that teaches it. The generator narrows where to look; it does not
decide.
