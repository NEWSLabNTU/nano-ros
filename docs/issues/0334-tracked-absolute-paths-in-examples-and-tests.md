---
id: 334
title: "Hardcoded build-host Zephyr-SDK path in the test harness (the last tracked absolute-path leak outside the SystemModels of #320)"
status: resolved
type: bug
severity: medium
area: testing
related: [issue-0320, rfc-0026]
---

## Finding (audit 2026-07-28, P2)

A repo-wide sweep established the **complete** set of tracked files carrying a
build-host absolute path. It is exactly two groups:

1. The committed `examples/**/config/*_model.yaml` SystemModels — **tracked by
   issue #320** ("Committed SystemModels are not self-contained"), filed
   concurrently by another session with a fuller diagnosis (`meta.record`
   pointing at vanished files, write-only `sha256`es, and the `system.toml`
   recording defeated). Not duplicated here.
2. This issue: one hardcoded SDK path in the test harness.

Everything else that matched (`examples/**/.cargo/config.toml`, `Cargo.toml`
walk-ups) is sanctioned `nros sync`-managed RFC-0048 W9 output, with the absolute
part living in the untracked `nros-patch.toml` — verified, not a finding.

## `packages/testing/nros-tests/src/zephyr.rs:417`

```rust
let qemu = std::env::var("QEMU_BIN").unwrap_or_else(|_| {
    let sdk = "/home/aeon/repos/nano-ros/scripts/zephyr/sdk/zephyr-sdk-0.16.8";
    format!("{sdk}/sysroots/x86_64-pokysdk-linux/usr/bin/qemu-system-xilinx-aarch64")
});
```

The `QEMU_BIN` fallback for `start_qemu_a9_mcast` (cortex_a9 / zynq7000s
multicast fixtures). Three problems in four lines:

- **Not portable.** On any other host `Command::new` fails with a bare ENOENT
  instead of a diagnosable "SDK not found" — while the **adjacent** dtb lookup
  (:419-424) already resolves properly through `zephyr_workspace_path()`. The
  correct pattern is 2 lines below the wrong one.
- **Second SSoT for the SDK version.** `0.16.8` is owned by
  `scripts/zephyr/setup.sh:78`; a bump there breaks this silently.
- **Second SSoT for the host tuple.** `x86_64-pokysdk-linux` is baked, so the
  fallback cannot work on an aarch64 dev host even with the SDK installed.

## Fix

`ZEPHYR_SDK_INSTALL_DIR`, else `project_root().join("scripts/zephyr/sdk")` (the
same `project_root()` used at :604), then **glob** `zephyr-sdk-*` and
`sysroots/*-pokysdk-linux` rather than pinning either. Return a `TestError` /
`nros_tests::skip!` when absent instead of handing a nonexistent path to
`Command`.

## Gate it (covers #320 too)

After both this and #320 land:

```
git grep -nE '/home/|/Users/' -- examples/ packages/
```

must be empty. The sweep confirms these two groups are the only sources, so the
gate goes green immediately and then stays honest — a CI grep is what would have
caught `bb0b08419` at review time.

## Resolved (2026-07-28)

`packages/testing/nros-tests/src/zephyr.rs` now resolves the binary instead of
naming it. Order mirrors the file's other resolvers: `ZEPHYR_SDK_INSTALL_DIR`,
else `project_root().join("scripts/zephyr/sdk")` — and inside a root, BOTH the
SDK version (`zephyr-sdk-*`) and the host tuple (`sysroots/*/usr/bin/`) are
globbed rather than pinned, since the issue named both as second SSoTs.
Multiple SDKs sort newest-first. Absence now returns
`TestError::BuildFailed` naming `QEMU_BIN` and `just zephyr setup`, instead of
handing a nonexistent path to `Command` for a bare ENOENT.

Verified by probing the resolver on this host: it derives
`…/zephyr-sdk-0.16.8/sysroots/x86_64-pokysdk-linux/usr/bin/qemu-system-xilinx-aarch64`
— byte-identical to the string that was hardcoded, but computed. (The probe was
scratch, not committed; the fixtures this feeds are cortex_a9 multicast lanes
that need the SDK to run at all.)

## Correction: the proposed gate does NOT go green immediately

This issue states the sweep found "exactly two groups" and that
`git grep -nE '/home/|/Users/' -- examples/ packages/` "goes green immediately"
once both land. Run after this fix, it returns **15 matches**, so the sweep was
incomplete in two ways:

- **13 hits in `packages/cli/docs/`**, missed entirely. Most are the legitimate
  placeholder `/home/user/project/...` in `CLI_REFERENCE.md`; the rest are
  `phase-1-idl-generator.md` pointing at `/home/aeon/repos/cargo-ros2/tmp/...`,
  a retired repo. A gate over `packages/` catches all of these, so it needs
  either a docs exclusion or those references cleaned.
- **2 SystemModels outside `examples/`** —
  `packages/testing/nros-tests/bins/{entry-poc,qemu-baremetal-main-e2e}/config/system_model.yaml`.
  This issue scoped #320's group to `examples/**/config/*_model.yaml`; these
  two live under `packages/testing/.../bins/` and belong to #320's population.
  Noted there.

What IS true: **zero absolute paths remain in source code** under `examples/`
or `packages/` — every survivor is a committed model (#320) or documentation
prose. So the gate is still the right idea; it just needs its scope stated
accurately before it can be switched on.
