---
id: 307
title: "Model-path parameter resolution diverged from rcl: a param file's sections merged in TEXTUAL order, and a float baked as an integer"
status: resolved
type: bug
severity: high
area: codegen, orchestration
related: [issue-0276, issue-0303, rfc-0050]
---

## Filed retroactively (2026-07-28)

Both defects were found and fixed on 2026-07-27 during the phase-296 residue
sweep, and recorded only in commit messages, RFC-0050 and the phase doc. They
SHIPPED and were user-visible on every model bake, so they belong in the issue
series — this file exists so the divergences are findable from here, not only
from a commit archaeology trail.

## Defect 1 — within-file section precedence was textual, not by specificity

rcl buckets a param file's entries per node and lets a node-specific block
override what `/**` set, **however the two are ordered in the YAML**.
`NodeInstance::resolved_params` merged matching sections in file order, so:

```yaml
/ctrl/planner:
  ros__parameters:
    rate: 25          # the node's own value
/**:
  ros__parameters:
    rate: 10          # written later, so it won
```

baked `rate: 10`. Any config that happens to write the wildcard block last got
the wildcard's value — silently, with the correct-looking file in front of the
user.

The rule HAD been implemented correctly, in nano-ros's
`orchestration::params::param_file_values` — but that copy served the launch
path, which phase-296 R-code retired. The copy that shipped (the model crate's)
never had it. Two implementations of one rule, and the survivor was the wrong
one. See RFC-0050 §"Semantics ship with the schema, not with each consumer".

**Fixed** in rlm `48e8d70`: sections merge pure-wildcard → partial-wildcard →
literal, stable within a rank. The duplicate matcher was deleted (nano-ros
`e51492f`), so there is one implementation left.

## Defect 2 — a launch double baked as an INTEGER

`ParamValue` → String was hand-copied three times (entry codegen,
`model_ingest`, `nros::main!`). `1.0f64.to_string()` is `"1"`, which the
runtime's `infer_param_value` re-types as INTEGER — so a parameter the launch
declared as a double reached the node as an int. The macro had been fixed and
carried a comment explaining the hazard; the other two copies still used
`to_string()`.

A node reading `ctx.parameter::<f64>("gain")` would fail its type check, or
worse, silently observe an int-typed parameter.

**Fixed** by `ParamValue::to_bake_string()` in the model crate (rlm `48e8d70`),
called by all three consumers.

## Why neither was caught

Nothing asserted parameter resolution against a running binary — every test
stopped at the plan or the projection. Both defects are now covered end to end
by `param_live_read_e2e`, whose fixture model encodes a three-way oracle (120
correct / 250 ordering lost / 999 specificity lost) observable on the wire, and
which was mutation-checked in both directions.

## Lesson recorded

A rule implemented twice will diverge, and the copy that ships is not
necessarily the copy that is right. RFC-0050 now states the ownership rule;
this issue is the evidence behind it.
