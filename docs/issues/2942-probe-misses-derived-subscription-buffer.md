---
id: 2942
title: "A subscription to a bounded message larger than the default executor buffer fails the metadata probe, before the image's derived buffer is applied"
status: open
type: bug
severity: medium
area: [cli, sizing, executor]
related: [phase-589, issue-2853]
found: 2026-11-24
---
## What
A Linux listener subscribing to a fixed-size `uint8[100000]` message stops
at the metadata probe, before any image build:

```text
metadata-mode harness failed (exit 101) for component 'listener':
error[E0080]: evaluation panicked: this message type's maximum serialized size
exceeds NROS_EXECUTOR_SUBSCRIPTION_BUFFER_BYTES
```

For the image itself the knob is derived over the linked closure
(`leaf_entity_env::DERIVED_CLOSURE_ENV_KEYS`), so the image build would size
the buffer. The probe compiles the same component with the crate default
(1024 B) and the compile-time check refuses. So any image that receives a
bounded message above the default fails on a knob the user should never have
to state. Setting `NROS_EXECUTOR_SUBSCRIPTION_BUFFER_BYTES` by hand gets past
it, as phase-589's 100 kB acceptance run had to (issue 2853).

## Fix direction
The probe build gets the same closure-derived subscription buffer as the
image, or the compile-time check is relaxed under the probe (`metadata-mode`),
where no sample is received.

## Acceptance
The 100 kB listener of issue 2853's acceptance builds with no
`NROS_EXECUTOR_SUBSCRIPTION_BUFFER_BYTES` in the environment.
