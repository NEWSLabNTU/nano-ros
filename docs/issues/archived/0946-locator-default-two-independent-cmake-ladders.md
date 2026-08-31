---
id: 946
title: "Two independent per-platform locator ladders in cmake, and they disagree on threadx and freertos"
status: resolved
area: build
severity: medium
found: 2026-08-31
related: [phase-405, RFC-0060, 0263]
---

# One compile definition, two producers, different values

`NROS_ENTRY_LOCATOR` — the address an embedded image dials — is computed by
**two** independent per-platform ladders that both write the same compile
definition:

| platform | `NanoRosEntry.cmake:555-562` | `NanoRosNodeRegister.cmake:535/637/741` |
| --- | --- | --- |
| threadx | `tcp/127.0.0.1:7447` (threadx-linux) | `tcp/10.0.2.2:7553` |
| freertos | `tcp/192.0.3.1:7447` | `tcp/10.0.2.2:7447` |
| nuttx | `tcp/10.0.2.2:7447` | `tcp/10.0.2.2:7447` |

`NanoRosEntry.cmake` even says so out loud:

> Mirrors NanoRosNodeRegister.cmake freertos branch; keep in sync

A hand-sync instruction with **no gate** — the same shape phase-405 W6 found for
the zenoh tx knobs, rotated: there the two spellings agreed and no direction was
declared; here the direction is declared informally and the values *disagree*.

## What is NOT yet known — and why this is filed rather than fixed

**Whether the disagreement is a bug.** Both branches carry a justification and
both are plausible:

* `NanoRosEntry.cmake:557-560` justifies freertos `192.0.3.1` by a static-lwIP
  network claim (issue 0263 C2b);
* `NanoRosNodeRegister.cmake:737-739` justifies `10.0.2.2` by "matches the
  qemu-arm-freertos example deploy".

Those are different networks, so it is entirely possible each ladder is correct
for the lane that reaches it. Deciding needs a QEMU run per platform, not a
grep, and merging two ladders that legitimately differ would break whichever
lane loses.

So the work is, in order:

1. determine per platform which lane actually reaches which ladder, by running
   the images — not by reading the call graph;
2. if the values should agree, merge into one `_nros_resolve_entry_locator()`
   helper (the phase-405 W3 `_nros_resolve_ros_edition()` shape) and delete the
   "keep in sync" comment along with the second ladder;
3. if they should NOT agree, say why in both files and add a gate asserting
   there is exactly ONE producer per (platform, lane) — the invariant is "one
   producer", not "equal literals".

Found by the phase-405 W3 config survey. W3 fixed the `ros_edition` instance of
this class (six defaulting sites → one) and left this one open precisely because
the edition sites were provably identical and these are provably not.

## Resolution — one producer, both answers preserved

Collapsed to a single `_nros_resolve_entry_locator(<lane> <platform> <board>
out_var)` in the new `cmake/NanoRosEntryLocator.cmake`, modelled on phase-405
W3's `_nros_resolve_ros_edition()`. The literals are `CACHE INTERNAL` because
`nano_ros_node_register()` includes the module from inside a function frame,
which is exactly where a plain `set()` at module top level goes missing (the
`_NROS_ENTRY_DIR` hazard; it cost W3 a debugging cycle).

**The values were NOT merged.** Step 3 of the plan above was the one taken: the
lanes disagree, both justifications name real and different networks, and
deciding between them needs a QEMU run rather than a reading. Each lane keeps
its own rung.

### Which lane reaches which ladder — MEASURED, not read

Established by configuring real projects with `-DNANO_ROS_ROOT` pinned at this
worktree, no `-D…LOCATOR` override, and reading the value back out of the
generated `build.ninja` / generated TU:

| lane | platform / board | locator |
| --- | --- | --- |
| entry | threadx / threadx-linux | `tcp/127.0.0.1:7447` |
| entry | threadx / riscv64-qemu | `tcp/10.0.2.2:7447` |
| entry | freertos | `tcp/192.0.3.1:7447` |
| entry | nuttx | `tcp/10.0.2.2:7447` |
| node-register | nuttx | `tcp/10.0.2.2:7447` |
| node-register | threadx | `tcp/10.0.2.2:7553` |
| node-register | freertos | `tcp/10.0.2.2:7447` |

The `entry` lane is `nano_ros_entry()`, reached directly by workspace Entry pkgs
and via `nano_ros_add_executable()` by every standalone
`examples/<plat>/<lang>/<name>`. The `node-register` lane is the three RTOS
typed-entry carriers in `nano_ros_node_register()`.

Before/after transcripts over all seven coordinates are byte-identical, and the
five override rungs (`-DNROS_ENTRY_LOCATOR`, `-DNROS_{NUTTX,THREADX,FREERTOS}_LOCATOR`)
were verified to still win.

### A finding: the node-register RTOS carriers are unreachable IN-TREE

Every in-tree `nano_ros_node_register` / `nros_components_register_node` call
site leaves DEPLOY empty — no keyword, and no `<export><nano_ros deploy=…>`
tuple on any workspace member's `package.xml`. The threadx and freertos carriers
require a non-empty DEPLOY and the nuttx one requires `nuttx IN_LIST`, so
nothing in this repository fires any of the three. They are live API for an
out-of-tree consumer package carrying a deploy tuple, which is what they were
written for; the measurement above used a probe package supplying exactly that.
Their values are therefore preserved rather than deleted.

Two stale claims surfaced while establishing this and are left as they are,
noted here rather than "fixed" on a reading: the nuttx carrier's docstring says
the `rtos_e2e` harness needs it, but those resolvers build
`examples/qemu-arm-nuttx/{c,cpp}/*`, which are lane A; and the threadx/freertos
carrier guards test DEPLOY for non-emptiness rather than for their own platform,
so any component pkg that ever gains a `DEPLOY native` would grow an RTOS
carrier. Neither is this issue's subject.

### Left alone deliberately

* **The two lanes still disagree on threadx and freertos.** That is the point of
  a lookup rather than a constant.
* **`NANO_ROS_PLATFORM` is compared RAW.** `nano_ros_entry()` normalizes
  `freertos_armcm3` → `freertos` for its DEPLOY membership test but *not* for the
  locator decision, so such a build takes the default rung. Normalizing here
  would silently move it from `10.0.2.2` to `192.0.3.1` — a different network.
* **The board-level `NROS_APP_CONFIG.locator` emitters** in
  `cmake/board/nano-ros-board-{threadx-linux,riscv64-qemu}.cmake` are a
  *different* symbol with a different consumer (`startup.c`), mirroring each
  board crate's Rust `build.rs`. Measured, they disagree with the compile
  definition on threadx — `7555` and `7553` against `7447` — and the generated
  template states `.zenoh.locator` is cosmetic on the typed path. They are
  allowlisted in the gate with that reason, not folded in.

### Gate

`check-entry-locator-ssot` (`scripts/check-entry-locator-ssot.py`, fast line).
Asserts one producer, not equal literals: no locator literal outside the SSoT
(bar the documented allowlist); every write of `NROS_ENTRY_LOCATOR` takes its
value from a `_nros_resolve_entry_locator()` out_var *in that file*; and no
"keep in sync" note about the locator returns.

The rule is checked **per write, not per file**. The first version was shell and
asked only whether a file mentioned the resolver anywhere — which passed a
planted regression that reverted two of three carrier writes to their own
defaults, because the third still called it. That is the issue-0196 shape (a
gate narrower than the rule it enforces) and is why the gate is Python: it reads
each write's right-hand side. Its selftest runs on the normal path and drives
`analyze()` itself rather than a re-typed copy of its regexes.
