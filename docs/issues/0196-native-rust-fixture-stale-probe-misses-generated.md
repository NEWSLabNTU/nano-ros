---
id: 196
title: "native rust fixture stale-probe misses generated/ drift — month-old museum binary passed every sweep"
status: open
type: tech-debt
area: testing
related: [issue-0181, phase-287]
---

## Summary

`examples/native/rust/service-client-callback` sat with a **June-12 binary**
(`target-zenoh/nros-fast-release/service-client-callback`) against a June-14
`generated/builtin_interfaces/src/lib.rs` — and every `just
build-test-fixtures` since (including two runs in the 2026-07-13/14 phase-287
W7 sweep, both "== native == OK") left it untouched. The TEST-side gate
(`require_prebuilt_binary` mtime check) correctly flagged it:

```
Test fixture is STALE — a source is newer than the built binary:
  binary: …/service-client-callback/target-zenoh/nros-fast-release/service-client-callback
  newer:  …/service-client-callback/generated/builtin_interfaces/src/lib.rs
```

so `test_service_callback_interop_rust_client_{c,cpp}_server` failed on every
full sweep while the build stage kept reporting green — the #181 "silent lane
gap" shape: build-side probe and test-side gate disagree about what "stale"
means.

## Root-cause direction

`scripts/build/fixtures-build.sh` (native rust cells) decides rebuild via the
`rust-fixture-stale.sh` probe + cargo's own freshness. Two candidate holes:
- the probe's input set doesn't include `<dir>/generated/**` (the codegen
  output that the test gate DOES watch), and/or
- `nros sync` refreshes `generated/` mtimes without changing content, cargo
  sees fingerprint-equal inputs and skips the relink, leaving the binary older
  than `generated/` forever after.

Either way the invariant should be: **whatever the test-side staleness gate
watches, the build-side probe must watch too** (single shared helper, like the
workspace-fixture signature dedup).

## Workaround

Force-build in the example dir (this is what un-stuck it on 2026-07-14):

```sh
cd examples/native/rust/service-client-callback
nros sync . && cargo build --profile nros-fast-release --target-dir target-zenoh
```

## Repro of the gap

Any full sweep before 2026-07-14: `just build-test-fixtures` (green) then
`cargo nextest run -E 'test(test_service_callback_interop_rust_client_c_server)'`
→ STALE failure.
