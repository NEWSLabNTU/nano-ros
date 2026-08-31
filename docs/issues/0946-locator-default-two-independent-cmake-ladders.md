---
id: 946
title: "Two independent per-platform locator ladders in cmake, and they disagree on threadx and freertos"
status: open
area: build
severity: medium
found: 2026-08-31
related: [phase-405, RFC-0060]
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
