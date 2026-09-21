---
id: 1262
title: "`nros setup --tool` records a tool in nros-sdk.lock only when it
  INSTALLS it, into whatever directory it runs from -- a project cannot get a
  complete lock for a shared store"
status: resolved
type: bug
area: tooling
severity: medium
related: [issue-1254, issue-1259, phase-447]
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

## Resolution (2026-09-21)

Both halves reproduced first, against a scratch store and a `file://` dist:

```
--- run 1: nros setup --tool widget, from the project ROOT ---
locks: $S/proj/nros-sdk.lock
--- run 2: nros setup --tool gadget, SAME project, from proj/sub ---
locks: $S/proj/nros-sdk.lock $S/proj/sub/nros-sdk.lock      # (b)
--- both tools in the store; ask again from the project root ---
nros setup --tool widget: present 1.0 (skip) -> $S/store/sdk/widget/1.0
nros setup --tool gadget: present 1.0 (skip) -> $S/store/sdk/gadget/1.0
locks:                                                      # (a) -- none
```

After: one lock, at `$S/proj/nros-sdk.lock`, naming both tools, whichever
directory the command ran from and whether or not it installed anything.

**(a) `Present` is recorded, deliberately.** The lock answers *what does this
project's toolchain set consist of*, not *what did this command do*. RFC-0014
§4 calls it "installed" against the index's "desired", and RFC-0095 made the
store shared, so "installed by me" and "installed in the store" stopped being
the same set. The provenance is not re-derived: `plan_install` returns
`Present` precisely because `<prefix>/.nros-provenance` was readable, so
`run_step` reads back the marker that decision was made on. `installed` stays
false — nothing was installed, and the "ready"/smoke lines key on that, not on
the lock. The sharper consequence of NOT recording it was not the committed
lock at all: `nros-sdk.lock` is one of `store::PIN_FILE_NAMES`, so an
unrecorded present tool is an entry `nros store gc` believes nobody pinned.

**(b) The lock is anchored, with one rule.** `store::lock_path_for(start)`:
the nearest ancestor of `start` carrying any pin file (`nros-sdk.lock`,
`nros-sdk-index.toml`, `nros-toolchain.toml`) is the project, and the lock
belongs beside them; failing that, `start` itself, which is self-correcting
because the lock it creates is a pin file. It must be the nearest DIRECTORY
rather than the nearest lock — a checkout nested inside another project sees
the outer lock up the path, and writing through to it would record the inner
project's toolchain in the outer one's file. `cmd/setup.rs`'s
`project_lock_path()` is the only place a working directory becomes a lock
path, for all three install paths (board, `--tool`, and the lazy
`ensure_tools` under `nros build`).

Two consequences handled: `SdkLock::save` is write-if-changed, because
recording present tools makes the lazy path reach it on every `nros build`;
and `nros setup --tool` now prints `locked in <path>`, since the board path
had said so all along and `--tool` said nothing, which is why a lock landing
in the wrong directory was invisible.

Tests (each verified to fail with its half of the fix reverted):

* `orchestration::store::tests::a_run_from_a_subdirectory_lands_on_the_project_s_existing_lock`
  and its three siblings — the derivation, `start` as a parameter so no test
  touches `set_current_dir` (issue 1101).
* `cmd::setup::tests::no_install_path_spells_the_lock_file_name_for_itself` —
  the CALL SITES. The derivation's own tests could not have caught this: the
  bug was three paths each spelling `PathBuf::from(LOCK_FILE)` inline, and
  fixing one and leaving two is the class CLAUDE.md warns about.
* `cmd::setup::session::tests::a_tool_already_in_the_shared_store_reaches_the_next_project_s_lock`
  — asserts both halves, because "it is in the lock" only means something if
  nothing was installed to put it there.
