---
id: 2686
title: "A truncated `additional_constraints` text is reported as a description that did not fit, naming the wrong knob"
status: open
type: bug
area: [params]
severity: low
found: 2026-11-18
related: [2650, phase-446, phase-417, phase-478]
---

Found while capturing descriptor prose lengths for [issue 2650](2650-param-descriptor-description-per-slot.md).

## What the code does

A descriptor's two prose fields have separate capacities: `description` is
`String<MAX_PARAM_DESCRIPTION_LEN>` (`NROS_MAX_PARAM_DESCRIPTION_LEN`, default
256), and `additional_constraints` is `String<MAX_PARAM_CONSTRAINTS_LEN>`
(`NROS_MAX_PARAM_CONSTRAINTS_LEN`, default 0; phase-417 W4.a gave it its own
knob).

The truncation report still treats them as one. In `nros-params`,
`ParameterDescriptor::set_additional_constraints` sets the same
`description_truncated` flag as `set_description`. Its comment reasons that
"the remedy -- raise `NROS_MAX_PARAM_DESCRIPTION_LEN` -- is the same for
either field, because they share the knob", which stopped being true when
constraints got their own knob. `Executor::report_truncated_descriptions`
(`nros-node/src/executor/spin.rs`) then logs, for either case:

> its description is longer than NROS_MAX_PARAM_DESCRIPTION_LEN (N bytes) ...
> Raise NROS_MAX_PARAM_DESCRIPTION_LEN to keep it whole

## Why it matters

The constraints default is 0, so ANY constraints text truncates on a default
build, from rclc's `rclc_add_parameter_description`, C++'s
`nros_cpp_node_add_param_description`, or the Rust builder's `.constraints(..)`.
The log then names the description, which may be intact, and tells the user to
raise a knob that does not change the field that was cut. Raising
`NROS_MAX_PARAM_DESCRIPTION_LEN` costs every slot RAM and leaves the
constraints truncated.

## Possible shape

Two flags, or a flag per field, and a report that names the field and the
knob that sizes it: `NROS_MAX_PARAM_CONSTRAINTS_LEN` for a constraints text.
