---
id: 25
title: host-integration lane fails native action/c_xrce/bridge tests — fixtures not staged
status: open  # fix applied, pending CI confirmation
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

**Scope is broad (updated 2026-06).** Not a handful of tests — the latest
`host integration-tests` run fails ~300 of 348 tests across many binaries
(`native_api` 56, `xrce` 40, `zephyr` 96, `multi_node`, `services`, `qos`,
`executor`, `custom_msg`, `platform`, `zero_copy`, `actions`, `c_xrce_api`,
`bridge_mixed_rmw`). They RUN and fail (not a compile error) — nearly every
integration test spawns a prebuilt example binary via `require_prebuilt_binary`
(`build_example_rmw` does not build, it looks up). The lane never stages any
fixtures (its only build prereq is `build-zenohd`).

**Blocked-by, now unblocked.** The fixture build the lane needs
(`build-test-fixtures` / `just native build-fixtures`) was deadlocking in
[issue 0022] (native-cyclonedds parallel corrosion→cargo); 0022 is now
**resolved**. So the lane can finally stage fixtures.

**Fix direction (multi-part — test-infra owner).** To green the lane:
1. Run the native fixture build (`just native build-fixtures`, host-only,
   now safe post-0022) before `test-integration` so the native example
   binaries the bulk of tests spawn are present.
2. Provision the micro-XRCE-DDS Agent (the `xrce` / `c_xrce_api` / bridge
   tests `require_xrce_agent`) or let those skip cleanly.
3. Exclude or skip the `zephyr`-binary group (96 tests) — it needs a Zephyr
   fixture/SDK absent in a host lane; it is not covered by the existing
   `group(=qemu-zephyr)` exclusion in the `test-integration` `-E` filter.

A partial fix (e.g. only #1) leaves the lane red, so this is a coordinated
change in the team's just-unblocked 0022 follow-up territory, not a surgical
one-liner.

**Fix applied (2026-06), mirroring the user workflow.**
1. `test-integration` recipe now tolerates `[SKIPPED]` (same contract as
   `_nextest-platform`): run, rewrite `[SKIPPED]` failures → `<skipped>`, pass
   iff no *real* failures. So the `skip!`-based tests — Zephyr (no SDK; #3),
   XRCE / c_xrce / bridge (no agent; #2) — skip cleanly instead of reddening
   the job (no `-E` exclusion needed).
2. `host-integration-tests.yml` builds the native fixtures the way a user does
   (`just native build-fixtures`) before `test-integration`, so the
   `require_prebuilt_binary` tests (native_api / services / qos / actions /
   multi_node / executor / …) RUN and pass; the XRCE agent build is
   best-effort. Toolchains/sources come from the existing `nros setup`.

Not verifiable in this dev env (slow lane, full ROS/AMENT setup); the
host-integration CI lane is the confirmation. Archive once green.

**Follow-up (2026-06): mbedtls gap in the fixture build.** The first attempt
greened the `[SKIPPED]` side but `just native build-fixtures` then failed:
`fatal error: mbedtls/entropy.h: No such file or directory` while building the
native **TLS** fixtures (`features=["link-tls"]`, `target-tls`). Root cause:
posix uses `mbedtls = "pkg-config"` (`zenoh_platforms.toml`) → build.rs
generates a `.pc` pointing at **system** mbedtls (`/usr/include`,
`/usr/lib/...`), but the CI base image had no `libmbedtls-dev`. (`nros setup
--source mbedtls` provisions the *vendored* submodule for embedded; the posix
pkg-config path ignores it, and the vendored branch isn't posix-viable — no
`mbedtls_config.h`.) Versions are compatible: the vendored submodule is
2.28.9, Ubuntu 22.04 `libmbedtls-dev` is 2.28, and zenoh-pico's `unix/tls.c`
supports 2.x. Fix: add `libmbedtls-dev` to the CI base image
(`ci/docker/ci-base/Dockerfile`) — the right place, since every lane that
builds the native TLS fixtures needs it. The base image auto-rebuilds on the
`ci/docker/ci-base/**` push; host-integration greens on its next run with the
new image.
