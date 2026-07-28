---
id: 320
title: "Committed SystemModels are not self-contained: absolute host paths silently defeat the `system.toml` recording, `meta.record` points at files that no longer exist, and the recorded `sha256`es are write-only"
status: open
type: bug
area: orchestration
related: [phase-315, rfc-0060, issue-0293, issue-0274, issue-0196]
---

## Finding (2026-07-28)

43 of the 64 committed `config/*_model.yaml` files under `examples/workspaces/`
bake **absolute paths from whichever machine last generated them**:

```yaml
meta:
  inputs:
  - path: /home/aeon/repos/nano-ros/examples/workspaces/rust/src/demo_bringup/config/system_model.record.json
    sha256: 2bb5d4f8…
  - path: /home/aeon/repos/nano-ros/examples/workspaces/rust/src/demo_bringup/launch/system.launch.xml
    sha256: f5a550ae…
  record:
    path: /home/aeon/repos/nano-ros/…/system_model.record.json
  resolver:
    tool: play_launch
    version: 0.8.2
```

A committed artifact referencing `/home/aeon/…` is reproducible on exactly one
checkout. This was found while fixing the `nros-launch-resolve` degrade
(phase-315); the degrade is what let these survive, but the absolute paths are a
separate defect with a separate fix.

Current resolver output is already clean — 0 of the 21 new-schema models contain
an absolute path (the only `/`-leading value is `to: /remapped_out`, a ROS topic
name). So this is legacy data plus three structural gaps that let it recur.

## Why the absolute paths are not cosmetic

`meta.inputs[].path` has exactly one consumer, and it is load-bearing:
`packages/core/nros-macros/src/main_macro.rs:878`

```rust
let system_toml_path = model.meta.inputs.iter()
    .filter(|i| Path::new(&i.path).extension().is_some_and(|e| e == "toml"))
    .find_map(|i| {
        let raw = Path::new(&i.path);
        let candidate = if raw.is_absolute() { raw.to_path_buf() } else { bringup_dir.join(raw) };
        candidate.exists().then_some(candidate)
    })
    .unwrap_or_else(|| bringup_dir.join("system.toml"));
```

The macro uses the recording to find the `system.toml` the model was actually
resolved against (issues 0274/0293). On any machine that is not the generating
one, `candidate.exists()` is false and it **silently falls back** to
`bringup_dir.join("system.toml")` — which is the per-target leak the recording
exists to prevent. No warning, no error; the recording just stops working.

## Three structural gaps

**1. Relativity is not structural — it is an accident of how nros invokes the
resolver.** `input_path_string` (`…/ros-launch-manifest/model/src/lib.rs:120`)
canonicalizes and then strips a base, and the base is derived by assumption:

```rust
// resolve/src/model.rs:155
let input_base = launch_path.and_then(|p| p.parent()).and_then(|p| p.parent());
```

i.e. "the launch file's grandparent is the bringup package". Absolute paths come
back whenever that assumption does not hold:

* `launch_path: None` → `input_base = None` → everything absolute (there is a
  test asserting exactly this at `model/src/lib.rs:1096`);
* a RELATIVE launch path — `nros-launch-resolve launch/system.launch.xml` makes
  `input_base` `Some("")`, `canonicalize("")` fails, and absolute strings go
  straight back in;
* a launch file not at `<bringup>/launch/<f>.launch.xml`;
* any input outside the base (a sibling-package include) stays absolute *by
  design* (`lib.rs:114`).

`nros sync` is safe only because `resolve_system_models` happens to pass an
absolute path (`ws.rs:442`). Nothing enforces that.

**2. `meta.record` is dead on both ends, and the files it names do not exist.**
The producer hardcodes `record: None` (`resolve/src/ros/model_builder.rs:832`);
the only reader is a golden round-trip test. `rg --files -g '*.record.json'`
returns **zero** files, so the 24 old models that carry a `record:` block point
at a path that cannot resolve on any machine, including this one. Its retirement
is documented (`cli/src/dump.rs:3`, `resolve/src/model.rs:21`) but the field and
the committed data outlived it. (Unrelated to the build-time
`nros-model-record.json` synthesized by `model_ingest::plan_record_from_model`.)

**3. The recorded `sha256`es are write-only, and mtime staleness watches a
different set of inputs than the hashes do.** Nothing anywhere re-hashes an
input and compares. `resolve_system_models` uses mtimes only:

```rust
// ws.rs:372
let stale = |model| model.mtime < max(launch/*.xml, system.toml).mtime;
```

The horizon is `launch/*.xml` + `system.toml` **on disk**. But `meta.inputs`
hashes more than that — included launch files in sibling packages, contract
manifests, the `--sched` platform file. An edit to any of those changes the
recorded hash's subject and never trips the mtime gate. This is the same class
as issue 0196: a staleness probe that watches fewer inputs than the build
consumes.

It is also why phase-315's "refresh or fail" fix does not repair the 43 legacy
files — they are not mtime-stale, so nothing asks them to regenerate.

## The self-containment question

Even with relative paths, the model is not self-contained: it names sibling
files and the macro opens one of them. Worth deciding explicitly rather than by
default, since the only thing the macro wants from `system.toml` is the
capability/param data. Two directions:

* **keep the reference, make it structurally relative** — smaller change; the
  model stays a pointer into its package, which is fine as long as the package
  travels as a unit;
* **inline what the consumer needs** — the model becomes genuinely
  self-contained and `meta.inputs` reverts to pure provenance, at the cost of
  duplicating a slice of `system.toml` into a generated file.

## Schema leniency compounds it

`SystemModel` has **no** `deny_unknown_fields` anywhere
(`ros-launch-manifest/model/src/lib.rs:43`). An old-schema file deserializes
cleanly and any genuinely unknown key is dropped in silence — so a schema
migration loses data without a diagnostic. Only `meta.version` is gated, and
only against being *newer* than `SCHEMA_VERSION`.

## Fix sketch

1. Make relativity structural — take the bringup package root as an explicit
   input to the resolver rather than inferring it from the launch path's
   grandparent, and reject (or diagnose) an input that cannot be made relative.
   Lives in the vendored `ros-launch-resolve`, so it is a submodule change.
2. Drop `meta.record` from the schema and from the committed files.
3. Make staleness content-addressed — compare `meta.inputs[].sha256` against the
   actual files, which both fixes the mtime/hash input-set mismatch and makes
   the 43 legacy models regenerate on their own.
4. Regenerate all committed models; add a gate asserting no committed model
   contains an absolute path.
5. Decide the self-containment question above and record it.

Steps 3 + 4 are the ones that clear the existing data; 1 stops it recurring.
