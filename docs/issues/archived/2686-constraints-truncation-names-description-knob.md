---
id: 2686
title: "A truncated `additional_constraints` text is reported as a description that did not fit, naming the wrong knob"
status: resolved
type: bug
area: [params]
severity: low
found: 2026-11-18
resolved: 2026-11-19
resolved_in: "fix(nros-params, issue 2686): a truncated constraints text is reported as itself, naming its own knob"
related: [2650, phase-446, phase-417, phase-478]
---

Found while capturing descriptor prose lengths for [issue 2650](../2650-param-descriptor-description-per-slot.md).

## What happened

A descriptor's two prose fields have separate capacities: `description`
(`NROS_MAX_PARAM_DESCRIPTION_LEN`, default 256) and `additional_constraints`
(`NROS_MAX_PARAM_CONSTRAINTS_LEN`, default 0; phase-417 W4.a). The
truncation report still treated them as one.
`ParameterDescriptor::set_additional_constraints` set the same
`description_truncated` flag as `set_description`, and
`Executor::report_truncated_descriptions` logged, for either case, "its
description is longer than NROS_MAX_PARAM_DESCRIPTION_LEN ... Raise
NROS_MAX_PARAM_DESCRIPTION_LEN".

The constraints default is 0, so on a default build ANY constraints text
truncates, whether it comes from rclc's `rclc_add_parameter_description`,
C++'s `nros_cpp_node_add_param_description` or the Rust builder's
`.constraints(..)`. The log then named an intact description and pointed at a
knob that costs every slot RAM and leaves the constraints cut.

## Fix

- **`DescriptorText`** (`Description` / `Constraints`) is always compiled in
  `nros-params`. Before, it existed only behind `descriptor-text-observer`;
  that module now re-exports it. It carries each field's `field()`, `knob()`
  and `capacity()`, so the field-to-knob mapping lives in one place.
- **One flag per field.** `ParameterDescriptor` keeps one truncation flag for
  each field. `set_additional_constraints` sets only its own flag, read with
  `constraints_truncated()`. `truncated_texts()` lists the fields that were
  cut.
- **The report.** `take_truncated_descriptions` calls its reporter once per
  truncated field, passing the `DescriptorText`. The executor's warning names
  that field, that knob and that capacity.

## Tests

- **New:** `types::issue_0323_tests::a_truncated_constraints_text_is_not_a_truncated_description`.
  - It failed on main: the description was flagged though it was intact.
  - It now checks that only the constraints field is reported, under
    `NROS_MAX_PARAM_CONSTRAINTS_LEN`, and that with both fields cut each is
    reported under its own knob.
- **Strengthened:**
  - `server::tests::test_parameter_description` and
    `typed::tests::builder_constraints_reach_the_descriptor_from_every_terminal`
    now also assert that the reported field is `Constraints`.
