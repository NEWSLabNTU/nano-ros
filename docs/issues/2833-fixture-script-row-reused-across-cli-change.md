---
id: 2833
title: "A `fixture-script` row that runs `nros build` is reused after a CLI change, because its signature hashes only the codegen fingerprint"
status: open
type: bug
severity: medium
area: [fixtures, build, cli]
related: [issue-0196, issue-1656, issue-2702]
found: 2026-11-22
---

## What happened

While mutation-testing the issue 2702 acceptance row
(`image_features_shared_configure_refused`, builder `fixture-script`), I
changed `cmd/build.rs` so it no longer passes `-DNROS_FEATURE_IMAGE`, ran
`just setup-cli`, then rebuilt the row. The fixture build printed:

```
reused .../build/compile-check-fixtures/image_features_shared_configure_refused — inputs unchanged (.inputsig)
```

The test passed against the recorded output of the UNMUTATED CLI. Once I
removed the row's `.inputsig`, the same mutant failed the test, as it should.

## Why

`cc_row_reusable` (`scripts/build/compile-check-fixtures.sh`) reuses a row
whose `.inputsig` is unchanged. The signature covers the manifest record, the
row's `dir`, its recorded dep closure, the CLI's CODEGEN fingerprint and the
launch resolver's identity. That is enough for a row whose only use of `nros`
is codegen.

A `fixture-script` row's `build.sh` can run any `nros` verb. This one runs
`nros build`, whose output depends on the whole CLI, not only on what it emits
as code. A CLI change that leaves the codegen fingerprint alone therefore
leaves every such row "fresh". The rows record stale verdicts, and the
staleness probe on the test side agrees with them, because it recomputes the
same signature (issue 0196's rule, applied to an input set that is too small).

## Fix

A row whose script runs the CLI keys on the CLI's SOURCE stamp, the one
`check cli-fresh` reads, as well as its codegen fingerprint. Either every
`fixture-script` row does (a script is opaque, so it can call anything), or
the row declares it. The first is safer: it fails toward rebuilding, never
toward reuse.

Sweep: every `builder = "fixture-script"` row in `examples/fixtures.toml`
whose `build.sh` invokes `nros`.
