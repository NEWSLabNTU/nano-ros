---
id: 2618
title: "The knob delivery check refuses a build after link: a list separator and an override row read as mismatches"
status: open
type: bug
area: [cli, build, knobs]
severity: medium
found: 2026-11-15
related: [phase-478]
---

**Owner:** unassigned. The derived-knob resolution is phase-530's work.
**Home:** phase-478 (found by the NXP island).

## Symptom

On main 31846e9a7c the island's build fails AFTER the link step, although the
ELF is produced. The knob delivery check reports two mismatches:

1. `NROS_SUBSCRIBED_TYPE_BOUNDS`: the derived value separates entries with `;`
   and the resolved value with `,`. The two lists carry the same entries, so
   this is a false mismatch.
2. A `max_publishers` override row is flagged, although an override is an
   allowed way to state that knob.

The exact refusal text has not been copied into this issue yet. Ask the island
for the full output before fixing it.

## Next step

Find the producer of each spelling. One of the two lists should change to use
the other's separator, or the comparison should parse both into a list. Then
make the check accept an override row whose value is the override. Add a test
for each case.
