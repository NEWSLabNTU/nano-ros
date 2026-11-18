---
id: 2305
title: "User-tier messages cite internal issue ids (`(issue 1352)`) where they should say what to do; there is no policy separating them from contributor-tier refusals"
status: resolved
type: tech-debt
area: cli, ux, logging, build
related: [phase-537, rfc-0117, rfc-0116, 2270, phase-523, issue-2443, issue-2547, issue-2551, issue-2581, phase-552]
found: 2026-11-06
resolved: 2026-11-18
resolved_in: "phase-523 W5: PRs #3593, #3617, #3621, #3628, #3666, #3698, #3734, #3740, #3750, #3773, #3780, #3784, #3786, #3787 (both ratchets to zero, then hard rules); runtime, script, compile-time and indirect text in #3789, #3790, #3792, #3794, #3804, #3815, #3831 (issues 2443, 2547, 2551)"
---

**Home: [phase-523](../../roadmap/phase-523-education-labs-adoption-probes.md) W5.**

## What it was

An ordinary `nros build` printed a cargo warning ending "(issue 1352)": an
internal tracker id in place of a remedy, on output a user without a checkout
reads. There was no rule saying which output is the user's, and no gate.

## Resolution

**The rule.** RFC-0116 D2 (consolidated in RFC-0124 Principle 5): the user
surface names what is wrong, where, and how to fix it, and never a phase,
issue or RFC id. User ruling, 2026-11-12, option A: this includes
build-stopping refusals (CMake `FATAL_ERROR`/`SEND_ERROR`, build-script
panics). Ids live in source comments (`// History:`, `# History:`) and `-v`
detail. The trailing "history: issue N" allowance first proposed here was not
taken.

**The gates, all at baseline zero:**

- `check-user-surface-ids`: the CLI's strings and rendered help, `board` and
  `rmw` list/show, `doctor --report`, the user book, build-script warnings and
  panic text, the colcon plugin, runtime logs and panic text of every package
  and example, user-run scripts, compile-time text, and text that reaches a
  print through a binding, table, field or one function call.
- `check-cmake-user-output-refs`: every default-level `message()`, including
  `FATAL_ERROR` and `SEND_ERROR`, and every custom-command `COMMENT`.

The surfaces this issue did not reach went to issues
[2443](2443-runtime-log-and-colcon-messages-cite-ids.md),
[2547](2547-user-surface-text-formatted-away-or-tabled-cites-ids.md) and
[2551](2551-user-surface-text-beyond-function-flow-cites-ids.md), all
resolved. Public API docs (`pub` rustdoc, installed headers) were
[issue 2581](2581-rustdoc-and-public-headers-cite-ids.md), resolved too; its
counted ratchet's last ids belong to issue 2554.
