---
id: 2833
title: "A `fixture-script` row that runs `nros build` is reused after a CLI change, because its signature hashes only the codegen fingerprint"
status: resolved
type: bug
severity: medium
area: [fixtures, build, cli]
related: [issue-0196, issue-1656, issue-2702]
found: 2026-11-22
resolved: 2026-11-23
resolved_in: "the change that archives this issue"
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

## Resolution (2026-11-23)

`compile-check-signature.sh` folds the CLI's source stamp (`nros source-stamp`,
the number `check cli-fresh` reads) into the signature of every
`fixture-script` row, beside the codegen fingerprint. All four rows key on it,
not only the ones whose `build.sh` names `nros`: the script is opaque, and the
error is toward a rebuild. The test-side probe (`compile-check-stale.sh`)
recomputes the same signature, so it calls such a row stale too.

The sweep found one more builder with the same shape. A `cmake-configure` row
over a workspace template (no root `CMakeLists.txt`, a `.colcon_workspace`) is
built with `nros configure` + `nros build`, so it keys on the stamp as well:
8 rows. A row over a plain `CMakeLists.txt` runs codegen only and keeps the
fingerprint alone. The west rows already keyed on the stamp (issue 2439), and
their signature bytes do not change.

Measured, with a comment appended to `nros-cli-core/src/cmd/doctor.rs` (it
moves the source stamp but not the codegen fingerprint):

| row | builder | main | this change |
| --- | --- | --- | --- |
| `image_features_shared_configure_refused` | fixture-script | same signature (the bug) | moved |
| `cpp_robot_entry` | cmake-configure, workspace template | | moved |
| `cpp_port_minimal_publisher` | cmake-configure, plain CMakeLists | | same |

