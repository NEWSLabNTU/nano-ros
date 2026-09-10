---
id: 1262
title: "`nros setup --tool` records a tool in nros-sdk.lock only when it
  INSTALLS it, into whatever directory it runs from -- a project cannot get a
  complete lock for a shared store"
status: open
type: bug
area: tooling
severity: medium
related: [issue-1254, issue-1259]
---

## Symptom

A downstream project (Autoware Safety Island) provisioned two Zephyr SDKs into
the store and ended up with a lock that names one of them:

```
[tool.zephyr-sdk]
version = "0.16.8"
provenance = "prebuilt"
sha256 = "cb4e4012..."
```

`zephyr-sdk-1-0-1` is missing. It was installed first from a different
directory (the nano-ros checkout, which got its own 328-byte
`nros-sdk.lock`), and when the project's provisioning recipe later ran
`nros setup --tool zephyr-sdk-1-0-1` from the project root, the CLI printed

```
nros setup --tool zephyr-sdk-1-0-1: present 1.0.1 (skip) -> ~/.nros/sdk/zephyr-sdk-1-0-1/1.0.1
```

and wrote nothing.

## Cause

`cmd/setup.rs`, the `--tool` path:

```rust
match action {
    InstallAction::Present => {}
    ...
    other => {
        let prov = execute(&other, ...)?;
        if prefix_override.is_none() {
            let lock_path = PathBuf::from(LOCK_FILE);
            let mut lock = SdkLock::load(&lock_path)?;
            lock.record(name, &prov);
            lock.save(&lock_path)?;
        }
    }
}
```

Two things combine:

- **`Present` records nothing.** The lock is written only on an actual
  install, so a tool that is already in the SHARED store never reaches the
  lock of the project now asking for it.
- **The lock path is relative** (`PathBuf::from(LOCK_FILE)`), so it lands in
  the current directory, not in the project the tool was provisioned for.

RFC-0014 section 4 states the contract the other way: "`nros-sdk.lock`
(committed per workspace) captures the resolved (tool, version, sha256,
provenance) actually in the store -- index = *desired*, lock = *installed*,
like Cargo.lock. A clone with the lock reproduces the exact toolchain set."
Since RFC-0095 made the store shared by every project on the host, "installed
by this command" and "installed in the store" are no longer the same thing,
and only the second is what a project's lock is for.

The result is a lock that a project should commit and cannot trust: committed
as-is it tells a clone the project needs one SDK when it needs two.

## Fix shape

- Record `Present` tools too, with the provenance the store entry already
  carries (`.nros-provenance`), so asking for a tool is what puts it in the
  lock.
- Resolve the lock path to the project the tool is for (the workspace root
  the rest of the CLI already discovers), not the current directory.

## Acceptance

- Two projects provisioning the same tool from the store both end up with it
  in their own `nros-sdk.lock`, whichever installed it first.
- Running `nros setup --tool` from a subdirectory of a project updates that
  project's lock, not a new one in the subdirectory.
