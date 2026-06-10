---
id: 25
title: host-integration lane fails native action/c_xrce/bridge tests — fixtures not staged
status: open
type: bug
area: build
related: [phase-230, issue-0022]
---

The `host integration-tests` lane (`just test-integration`) fails a cluster of
tests that need pre-built native example binaries (action client/server,
C-XRCE listener/talker/service, zenoh↔xrce bridge). They **pass locally** but
fail fast in CI.

**Symptom** (`nros-tests::actions`, `::c_xrce_api`, `::bridge_mixed_rmw`):

```
FAIL [0.159s] test_action_client_starts
FAIL [0.007s] test_c_xrce_listener_starts
FAIL [0.156s] test_zenoh_to_xrce_bridge_e2e
```

The c_xrce cases fast-fail at ~6 ms (binary lookup), the action/bridge cases
at ~0.15 s. Locally `test_action_client_starts` PASSES (3.26 s) because the
native fixture binary is already built; in CI it is not.

**Cause.** `test-integration`'s only build prereq is `build-zenohd`. The
action/c_xrce/bridge tests resolve native example binaries that the lane never
stages (`build-test-fixtures` / the native example builds are a separate,
heavier step). zenohd provisioning was fixed separately (the lane now shows
`zenohd present`), which unmasked this fixture gap.

**Fix direction.** Either (a) build the needed native fixtures in the
host-integration lane before `test-integration` (scoped to the native cells,
not the full embedded `build-test-fixtures`), or (b) have the affected tests
`nros_tests::skip!` when their fixture binary is absent (matching the repo's
skip convention) so the lane stays light. A design call for the test-infra
owner; overlaps the in-flight fixture-build work in [issue 0022].
