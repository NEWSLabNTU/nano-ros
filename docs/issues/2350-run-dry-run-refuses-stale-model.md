---
id: 2350
title: "`nros run --dry-run` says it would configure, then refuses the stale SystemModel"
status: open
type: bug
area: cli-run
related: []
found: 2026-11-07
---

**Home: none yet.** Found replaying Tutorial 9 (`tutorial/another-target.md`)
on main e55d53507b with the release asset of an earlier main.

## Symptom

After the chapter adds a second `[image.freertos]` to `system.toml`, its
step `nros run baremetal --dry-run --why` prints:

```text
nros build: --dry-run: would configure sensor_node_bringup:baremetal: inputs changed (recorded sha256:86ab…, now sha256:f5d5…)
nros build:   resolved → /root/sensor_ws/build/sensor_node_bringup__baremetal/resolved.toml (entity counts not derived; configured defaults kept)
nros run: `baremetal`: SystemModel `/root/sensor_ws/build/nros/models/sensor_node_bringup/system_model.yaml` is stale: input hash changed `system.toml`
  Run `nros configure` to re-resolve it.
```

and exits non-zero. The `--why` lines naming the image never print.

## Cause

Under `--dry-run`, the implicit build reports that it *would* configure, and
so writes nothing. Then the run's model gate (`nros-cli-core/src/model_gate.rs:180`)
checks the model the dry run did not refresh, and refuses. A real `nros run`
would configure first and pass the gate.

## Acceptance

- Under `--dry-run`, a model that the implicit configure would refresh is
  reported as "would re-resolve", not refused.
- `--why` answers which image resolved without a configured model, since
  resolving the image name reads only `system.toml`.
- A CLI test edits `system.toml` after a configure, then asserts that
  `nros run <image> --dry-run --why` exits 0 and prints the `image =` lines.
