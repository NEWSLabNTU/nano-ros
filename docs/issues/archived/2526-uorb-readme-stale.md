---
id: 2526
title: "The uORB backend README still described the K.4.0 scaffold"
status: resolved
type: tech-debt
severity: low
area: [rmw, px4]
related: [phase-555]
found: 2026-11-14
---

**Home: `packages/rmw/uorb/nros-rmw-uorb/README.md`**

## What was wrong

The README's status section had been updated, but its opening and layout had
not:

- the opening called the backend a port of the Rust `nros-rmw-uorb` "slated
  for retirement once this backend reaches parity". The Rust backend and
  `nros-px4` are gone;
- the layout marked the session, publisher and subscriber sources "stubs in
  K.4.0", and listed none of the files added since: `qos.cpp`,
  `topic_registry.cpp`, `px4_callback_glue.cpp`, `callback_default.cpp`,
  `register_fallback.c`, `uorb_abi.hpp`, the registry header, the backend
  descriptor and the two cmake files;
- the build section said "Inside a PX4 module (K.4.1 onward)".

## Found by

phase-555's design review (rmw-wire brief, "Untracked work found"). The user
ruled that the untracked items be filed as issues (2026-11-14).

## Resolution

Fixed in the same commit that filed this issue: the opening says what the
backend is, the layout lists the files that exist (`git ls-files
packages/rmw/uorb/`), and the build section drops the phase step.
