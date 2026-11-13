---
id: 2520
title: "A probe failure that leaves an image on default sizes is only a WARNING, so a fixture build stays green while it ships the wrong sizes"
status: open
type: bug
severity: medium
area: [sizing, tooling, testing, build]
related: [issue-2519, issue-1593, issue-2097, issue-1758]
found: 2026-11-14
---

**Home: the C/C++ metadata probe (`orchestration/metadata_refresh.rs`) and the
fixture builders (`scripts/build/{fixtures,workspace-fixtures,compile-check-fixtures}-build.sh`)**

## Symptom

On main, the tier-2 nightly on 2026-11-12 and on 2026-11-13 built
`riscv64_threadx_c_typed_carrier` and `nuttx_c_typed_carrier` with

```
sync: source metadata — no producer for … (probe failed at this source last sync; …)
nros build:   resolved → … (entity counts not derived; configured defaults kept)
```

The lane was green both nights. The images ran on DEFAULT pool sizes, not
sizes derived from what the components declare. Issue 2519 fixed the cause
of that probe failure (a probe inheriting a cross configure's `CC`). It also
turned the failure from a cached quiet line into a loud WARNING. A warning in
a CI log is still green, and green is the only thing a nightly reports. So the
next cause of a whole-project probe failure will ship the same way.

## Why a warning is right for one caller and wrong for the other

- **An interactive `nros build` / `nros configure`.** A developer reads the
  warning, and refusing would block a build that does run. Keep the warning.
- **A fixture build.** It is the evidence that an image was built as its
  declarations say. An image on default sizes is not the image the fixture
  claims to be, and nobody reads the log of a green lane. This is issue 1758's
  rule one layer down: a missing precondition is a red, never a pass that
  prints something.

## Proposal

1. Add a declared strictness for the probe:
   `NROS_PROBE_FAILURE = warn | refuse`, default `warn`. Register it in the
   knob registry so it has one spelling (`knobs.toml`, `scripts/gen-knob-registry.py`).
   - A whole-project configure failure under `refuse` exits non-zero. The
     refusal names the batch and CMake's error (`first_error_line`), and says
     that the image would otherwise be built on default sizes.
   - Per-component failures keep their marker. Under `refuse` they also
     refuse (see the ruling below).
2. Every fixture builder sets `refuse`: `fixtures-build.sh`,
   `workspace-fixtures-build.sh` and `compile-check-fixtures.sh`. It is
   DECLARED by the lane, never inferred from `CI` or from a TTY, so that a
   local run of the same lane is as red as the remote one.
3. Sweep the other "configured defaults kept" paths in the same change, not
   only the probe batch, so the class is fixed rather than one site. Each place
   where a resolve proceeds with `entity counts not derived` or
   `message bounds not seeded` asks the same strictness. A fixture row that
   legitimately has no inventory (the `entity-census take … nothing to take`
   rows for an owned entry with no component) must say so in its manifest row,
   so that `refuse` is not answered by a blanket exemption.
4. Gate: a selftest that runs a fixture builder against a probe project that
   cannot configure, and asserts a red with the reason in its output. It is the
   negative control for step 2.

## Ruling on per-component failures

Per-component probe failures also refuse in a fixture build. A component
on default sizes is the same silent risk at a smaller scale. A component
that legitimately has no inventory says so in its manifest row, under the
same no-inventory rule as step 3, so `refuse` is never answered by a
blanket exemption. Under `warn` nothing changes. Under `refuse` the
per-component arm refuses too, and its marker still keeps the reason.

This is the manager's call (2026-11-14). The maintainer has not ruled on
it. Implementation follows phase-550 W3b.
