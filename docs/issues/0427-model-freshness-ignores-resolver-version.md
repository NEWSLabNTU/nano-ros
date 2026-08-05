---
id: 427
title: "a SystemModel is 'fresh' when only the RESOLVER changed, so a resolver fix never reaches existing models"
status: open
type: bug
area: build
related: [phase-330, phase-336, issue-0382, issue-0285]
---

## Symptom

`cpp_multi_node_entry::multi_node_workspace_cpp_typed_configures_and_builds`
fails:

```
component order doesn't match launch XML
```

The generated TU constructs `listener` before `talker`:

```cpp
static ::listener_pkg::Listener __nros_comp_0;
static ::talker_pkg::Talker     __nros_comp_1;
```

while `demo_bringup/launch/system.launch.xml` declares `talker` first.

## Cause

Not codegen, and not a stale fixture. The TU was regenerated today; it faithfully
emitted the order its INPUT carried. The input — the resolved SystemModel — is
what is stale:

```
build/nros/models/demo_bringup/system_model.yaml   mtime 2026-08-04 02:22
  structure.nodes:  /listener, /talker            # alphabetical: pre-fix order
  meta.resolver.version: 0.1.0
```

`ros-launch-manifest` v0.1.4 (`62e90af`, "structure.nodes keeps declaration
order — IndexMap, not BTreeMap") fixed exactly this: a `BTreeMap` alphabetized
init order away, so a launch declaring `talker` first produced an entry that
built `listener` first. nano-ros picked that up when the play_launch pin moved
to v0.1.4 (`82c88ef53`), and the resolver binary here was rebuilt afterwards
(2026-08-05 15:17). Resolving fresh produces the right order — verified
directly.

But `nros sync` will not re-resolve. The model records its inputs:

```yaml
meta:
  inputs:
  - path: launch/system.launch.xml
    sha256: e6b6…
  - path: system.toml
    sha256: f34e…
  resolver:
    tool: ros-launch-resolve
    version: 0.1.0
```

Neither input changed, so the model reads as fresh and sync exits 0 having done
nothing. **The resolver version is RECORDED in `meta.resolver` but is not part
of the freshness decision.** A resolver bug fix therefore never reaches any
model that already exists — on any developer machine, on CI, in any workspace
whose launch files happen to be stable.

`verify_resolver_pin` (ws.rs:1103) is a different check: it compares the
resolver BINARY against the pin nano-ros was built with, and deliberately
proceeds when either side is unverifiable. It says nothing about the model on
disk.

## Why it matters more than one test

The model is the input to every entry's codegen, and node order is semantic —
it fixes `__nros_comp_N` and the construct/`configure()` sequence in every typed
C++ entry. So the class is: *any* resolver change (ordering, params, remaps,
tiers) silently fails to propagate to existing models. The failing test is the
only place that noticed, and only because it asserts order explicitly.

It also fails safe in the wrong direction: sync exits 0, so nothing reports that
the model on disk was produced by a resolver that no longer exists.

## Fix

Make the resolver identity a freshness input, alongside the file hashes:

- `meta.resolver` already carries `tool` + `version`. Record the pin
  (`NROS_PLAY_LAUNCH_SHA`, the same value `verify_resolver_pin` compares) and
  treat a mismatch as stale, exactly as a changed input hash is.
- The recorded `version: 0.1.0` is itself suspect — the resolver is at v0.1.4
  and stamped 0.1.0, so whatever writes that field is not reading the real
  version. Fixing the staleness check without fixing the stamp would compare
  two constants.

Until then the workaround is to delete the model and re-sync:

```bash
rm -rf <ws>/build/nros/models && nros sync
```

Verified end-to-end 2026-08-05: after that, the model carries `/talker` first,
the regenerated TU constructs `talker` as `__nros_comp_0`, and
`cpp_multi_node_entry` goes 4 passed / 0 failed.

## Notes

Found triaging issue 0422 (the runtime E2E failure set) after phase-336. The
ordering CHANGE is correct and intended — see issue 0382 upstream — this is
purely about the change not reaching models that already exist.
