---
id: 1276
title: "`provision_source` panics on a `location = \"store\"` source — `nros
  setup rv-virt-threadx --rmw cyclonedds` dies at `expect(\"clone mode has a
  dest\")` on any host that does not already have rosidl"
status: resolved
type: bug
area: cli, build
severity: high
found: 2026-09-11
resolved: 2026-09-11
related: [rfc-0095, issue-1264, issue-0628]
---

## What this is

On `main`, `orchestration/sdk_store.rs`, `SourceProvision::Clone`:

```rust
let git     = src.git.as_deref().expect("clone mode has a git url");
let git_ref = src.git_ref.as_deref().expect("clone mode has a ref");
let dest    = src.dest.as_deref().expect("clone mode has a dest");   // line 980
```

And on `main`, `nros-sdk-index.toml`:

```toml
[source.rosidl]
version = "humble-5621b26"
git = "https://github.com/ros2/rosidl"
ref = "5621b26eff54596a356ad8abd72c06e7650d21f7"
location = "store"
```

No `dest`. Phase-440 / RFC-0095 D1/D2 introduced `location = "store"` and the
`source_dir()` helper that derives `$NROS_STORE/sources/<name>/<version>` for
exactly this case — but the `Clone` arm was never taught about it and still
requires the workspace-relative `dest` that a store-located source no longer
has.

`rosidl` is in `[rmw.cyclonedds].packages`, so every board that provisions
Cyclone reaches it.

## Reproduction

```
nros setup rv-virt-threadx --rmw cyclonedds
```

on a host where rosidl is not already provisioned:

```
nros setup: rv-virt-threadx (rmw cyclonedds) needs 7 package(s):
  riscv-none-elf-gcc     present 14.2-nros1 (skip)
  qemu                   present 11.0.0-nros6 (skip)
  threadx                source 6.4.1 — submodule … [provisioned]
  threadx-netxduo        source 6.4.x — submodule … [provisioned]
  cyclonedds             present 0.10.5-nros1 (skip)
  cyclonedds-src         source 0.10.5-nros1 — submodule … [provisioned]

thread 'main' (6583) panicked at nros-cli-core/src/orchestration/sdk_store.rs:980:44:
clone mode has a dest
error: recipe `setup` failed with exit code 1
```

Six of seven packages provision, then it panics on the seventh. `just setup
threadx_riscv64` fails with it.

## Why nobody has hit it

A host that provisioned rosidl before the `location` change has the tree already,
and the `present` check returns `AlreadyPresent` — but only AFTER the `expect`,
so that is not what saves it. What saves an existing host is that the OLD index
had `dest = "third-party/ros/rosidl"`; the panic needs the NEW index, which
means a fresh provisioning run against current `main`.

That is a contained self-hosted runner bootstrapping from an empty store, and
nothing else here does it routinely. The same wall waits for any new contributor
and any fresh CI image.

## Fix

In `Clone` mode, resolve the destination the way `source_dir()` already
documents:

* `location = "store"`  -> `source_dir(index, name)` — derived, never authored;
* `location = "workspace"` (the default) -> `workspace.join(dest)`, as today.

And replace the three `expect`s with errors that name the source and the missing
key. A panic here reports a Rust invariant to a user who typed a provisioning
command; the index is user-editable, so a malformed entry is an input error, not
an unreachable state. Every neighbouring failure in this file already does that
(`nros setup: {name} … has no prebuilt for {host} and no source recipe …`).

Note `SourceProvision::Clone` is selected before location is consulted at all —
check whether a store-located source should take the clone path in the first
place, or whether the mode selection needs the same fix one level up.

## Sweep

`location` was added by phase-440; the `Clone` arm is one consumer of
`SourceSpec` and there may be others that read `dest` unconditionally. Grep for
`src.dest` / `\.dest` across the CLI and fix them together rather than only the
site that panicked (the issue-0196 rule).

## Resolution (2026-09-11)

`provision_source`'s `Clone` arm asks `source_dir_of`, the derivation whose own
doc comment already claimed the installer as one of its three consumers. The
resolver MOVED from `cmd/setup.rs` to `orchestration/sdk_store.rs`, beside the
store root it derives from — `orchestration` reaching up into `cmd` for a path
is how the third consumer gets missed again. The two remaining `expect`s became
errors naming the source and the missing key.

The sweep found the display half: `describe_source` and the auto-setup notice
read `dest` directly and printed `-` for a store source, so the line said rosidl
would be provisioned to nowhere. Both take the derivation now. `sdk_index.rs`'s
reads are the validator itself; `metadata_build.rs` and `sdk_path.rs` degrade
rather than panic.

Verified three ways: a regression test for the shape (store source, no dest,
dry-run) that also asserts the path lands UNDER the store at
`sources/<name>/<version>`, since a fix that invented a workspace path would
pass a weaker assertion; a positive control, reverting the resolution line to
watch that test panic at the reported site; and the command itself against the
shipped index and an empty store, where all seven packages now resolve.
