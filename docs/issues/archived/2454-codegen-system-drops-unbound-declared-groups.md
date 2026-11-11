---
id: 2454
title: "codegen-system drops a node's declared callback groups that its authored group_tasks does not bind, so it feeds the derivation a different map from codegen entry for the same model"
status: resolved
resolved: 2026-11-12
resolved_in: "branch fix-codegen-system-partial-groups"
type: bug
severity: low
area: [scheduling, codegen, cli]
related: [2432, 1426, rfc-0079, rfc-0047]
found: 2026-11-12
---

Found while fixing [issue 2432](2432-derived-sched-tasks-differ-by-language.md). It is the
same class: the derived schedule must not depend on which road builds the
image. The user's 2432 ruling (2026-11-12) was "the schedule should not depend
on language choice". `codegen-system` and `codegen entry` are two C roads over
one model, so they fall under it too.

## Measured

2026-11-12, branch `fix-2432-rust-derived-sched` (a3b93518d7).

Take a node `ctrl_node` whose code declares two callback groups, `main` and
`telem`: the cmake `CALLBACK_GROUPS main telem`, carried in
`nros-metadata.json`. Its `[[component]]` authors one binding,
`group_tasks = { main = "high" }`. The per-node callback-group map, the
derivation's third argument, came out as:

| Road | Row |
|---|---|
| `codegen entry` (`entry_groups::node_callback_groups`) | `main -> high`, `telem -> default` |
| `codegen-system` (`sched_task_resolver::collect_callback_groups`) | `main -> high` |

So `telem` disappeared from the bake's input. `collect_callback_groups` took an
authored `group_tasks` as the WHOLE declared set (`if !c.group_tasks.is_empty()
{ ...; continue; }`), and consulted the keyword only for a component with no
binding at all. The two roads agreed only when a node bound every group it
declared, or none of them.

Nothing reported the difference. Worse, it was PINNED on purpose: the
function's own test, `an_authored_binding_still_outranks_the_keyword`, asserted
`decls.len() == 1`. 2432's resolution recorded the divergence and left it.

Why this is a defect and not a precedence choice: CLAUDE.md's issue-1426 rule
says precedence is **per FACT, never per table**. An authored binding decides
where the groups it NAMES run. It is not a statement that the node has no other
groups. An unbound declared group sits on `DEFAULT_SCHED_TASK`, which is
`derive::placement_is_unauthored` rule 4: exactly the input the rate-monotonic
derivation (RFC-0079 §3) keys on. Dropping it hides that group from the
derivation on one road only.

## Resolution

`collect_callback_groups` builds each component's row with
`nros_orchestration_ir::entry_groups::node_callback_groups(declared,
group_tasks)`, the row builder `codegen entry` and `nros::main!` already use.
Declared groups come from the keyword and bindings from `group_tasks`, merged
per group. With nothing declared, the bound groups are the node's groups, as
before. So every case that agreed before is byte-identical, and every existing
`derived_sched_tasks_*` test passes unchanged.

The pinning test is replaced by `an_authored_binding_decides_its_own_group_only`.
It asserts the partial-binding row `[(main, high), (telem, default)]`, and that
the row EQUALS what `node_callback_groups` gives the entry for the same input.
Before the fix it fails, because the old code returns one row. The
`entry_groups` module doc no longer describes a divergence.
