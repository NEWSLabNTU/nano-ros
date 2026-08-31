# Phase 405 — one configuration surface: name the SSoT, project the rest, delete the strays

**Status (2026-08-31). W1, W2, W4, W5 and W6 landed; W3 open.**
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

**W4 LANDED — eleven keywords to six, and the ordering constraint held.**
`MODEL` is gone: zero callers passed it, authored OR generated, and it named a
resolved artifact directly while `BRINGUP`+`LAUNCH` name the INPUT that produces
one. `_NRA_MODEL` is now written only by launch resolution — one way in.

`LAUNCH` stays PARSED and stops being required. 18 generated CMakeLists pass a
real launch file and a generator should be explicit; what changed is that a
human need not write it. All nine authored entries said `LAUNCH default`, which
is what `BRINGUP` alone now means, and all nine dropped the line.

**The default is conditional on `BRINGUP`, deliberately.** Defaulting it
unconditionally would turn an entry with nothing at all — a typo, a half-written
CMakeLists — from the existing "LAUNCH or SOURCES required" error into a
silently accepted launch-addressed entry. That is the caution issue 0931
recorded, and it is why the condition exists.

`DEPLOY` and `BOARD` are NOT removed. The ordering constraint below is real: the
gate at `NanoRosEntry.cmake:159` runs before the model is resolved, so making
them derivable needs the gate moved, which is its own change with its own
failure mode on the embedded path. Six keywords, not five, and the last two are
a separate decision.

Verified by CONFIGURING six workspaces (c, cpp, mixed, features, realtime-c,
realtime-cpp) rather than by grep — the fast tier never runs cmake, so a keyword
change that passes it proves nothing.

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

**W6 LANDED — one pair was already finished, the other had two checkers.**

The framing this doc opened with was wrong on both halves, and measuring it is
what produced the work.

*"SDK roots exist four times"* — no. The VALUE lives once, in
`just/sdk-env.just`. `activate.sh` deliberately sets none (activate.fish:152
says so), `.env.example`'s entries are commented placeholders. The other
mentions are a board→var binding table, its rendered output, and the cmake
readers. That is a generator with committed output plus a staleness gate —
the `check-abi-bindings` shape this repo already treats as an end state. Closed
as correct, no file changed.

*"the zenoh tx trio … asserted-equal by a gate rather than merged"* — the
direction was already DECLARED, in the descriptor's own comment since
phase-290, and it points the other way from where a naive reading lands: the
TOML is the authority and Kconfig's `default` lines mirror it. Worse, the pair
had **two** checkers —
`nros-tests/tests/kconfig_platform_default_drift.rs` (80 lines, declares the
direction) and `scripts/check-zephyr-knob-agreement.py` (140 lines, declared
none and called itself *"not a substitute for merging the sources"*). Same two
files, same three pairs, contradictory prose, for two phases.

So W6 was not a merge. It was: declare the direction in the one surviving
checker, give it `--write` so the mirror is GENERATED rather than asserted,
mark the three Kconfig lines as derived, and delete the duplicate checker. The
gate keeps the python one because it runs on `check fast` — the pre-push hook —
where the Rust test ran only in `test-unit`, and because a generator belongs
beside the gate that enforces it.

**Issue 0940 was 12 blocks, not 1.** The sweep found unreachable
`[deploy.<t>.nros]` site config in 7 bringups across 6 workspaces, not only in
`mixed`. Ten of the twelve carried content byte-identical to a live
board-named sibling in the same file, which is the only reason it had cost
nothing; `rust`'s two had no sibling but that workspace has zero
`nano_ros_entry` DEPLOY tokens, so nothing reached them either.

The generator was NOT the source — `check-site-config.py` keys on `board` and
`continue`s past a boardless target, so it never emitted these and never
checked them. That blind spot is the defect: S4 now reports a site block whose
target names no `board`, since board-facts resolution requires one by both of
its paths. The 12 blocks are deleted (-91 lines).

Both scripts gained a selftest on the normal path and left
`.config/gate-selftest-baseline.txt` (173 gates, 58 now self-testing).

`--write` was verified by round-trip, not by reading it: perturb a Kconfig
default, confirm the gate reddens, `--write`, confirm green and the file
byte-identical to the original.

Issue 0941 carries the remaining half — `nros_resolve_board_facts` still fails
SOFT, so the next unreachable block is still silent. Split deliberately: that
needs an enumeration of which configures legitimately resolve nothing, and
should not hold up the mechanical fix.

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
