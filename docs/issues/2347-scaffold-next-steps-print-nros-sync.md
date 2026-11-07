---
id: 2347
title: "`nros new` prints `nros sync` in a Rust package's next steps; the verb is `nros configure`"
status: open
type: bug
area: cli-scaffold
related: [issue-2346, phase-523]
found: 2026-11-07
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md).** Seen in the same checkout-track log as issue 2346.

## Symptom

A scaffolded Rust package prints:

```text
nros sync    # message crates + build/<image>/nros-cargo.toml   (should read: nros configure)
```

`nros sync` became an old spelling of `nros configure` on 2026-11-04.

## Cause

`packages/cli/cargo-nano-ros/src/scaffold.rs` `next_steps()` (line 440 on
main 50b443bd3f) still pushes the old verb, and its doc comment (line 422)
names it. `just check cli-old-spellings` does not scan the CLI's own
printed strings, so the gate missed it. Other comments and help strings in
`scaffold.rs` (lines 333, 551, 660) and `provider_scan.rs:1385` (a printed
remedy, "re-run `nros sync`", should be `nros configure`) carry the same old verb.

## Acceptance

- `next_steps()` and every printed remedy say `nros configure`.
- `cli-old-spellings` scans the CLI's printed strings, or a unit test
  asserts that `next_steps()` names no old spelling.
