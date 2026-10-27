---
id: 2097
title: "A metadata probe that failed on an unprovisioned source stays skipped after `nros setup --source`: the image is sized without its sidecars (island DTCM +31,416 B)"
status: resolved
type: bug
area: [build, cli, memory]
severity: medium
found: 2026-10-27
resolved: 2026-10-27
resolved_in: "PR pending"
related: [phase-478, 0288, 0390, 0641, 1469, 1578, 1593, 2070]
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
(what a refusal tells the person building the image to do).

## Measured

The Autoware safety island (MR-CANHUBK344, `mr_canhubk3/s32k344`) was moved
from nano-ros `fb3e6de4ed` to a later main in a fresh worktree. The board link
failed:

```
ld.bfd: zephyr/zephyr_pre0.elf section `.dtcm_bss_reloc' will not fit in region `DTCM'
ld.bfd: region `DTCM' overflowed by 2548 bytes
```

`NROS_CPP_LEVELS_BACKING_SIZE` had gone from 33,176 to 64,592 B, and the
change was first blamed on [issue 2070](2070-param-declare-stack-cost.md)
(`a54a70edbd`, the commit right after `fb3e6de4ed`). It is not 2070's. The
same island tree built at `a54a70edbd` itself, in a worktree whose sync probed
cleanly, gives:

| nano-ros | sync | `NROS_CPP_LEVELS_BACKING_SIZE` | DTCM |
| --- | --- | --- | --- |
| `fb3e6de4ed` | 4 rebuilt | 33,176 B | 102,204 B |
| `a54a70edbd` (#2070) | 4 rebuilt | 33,176 B | 102,204 B |
| later main, first worktree | probe failed, then skipped | 64,592 B | overflow by 2,548 B |

2070 changes no type that the executor backing is sized from (the parameter
slot, the arena model and the tier table are all unchanged).

## Cause

The overflowing worktree's first `nros sync` ran before zenoh-pico 1.10.1 was
in the SDK store. Every C/C++ probe failed in `zpico-sys`'s build script:

```
zenoh-pico source not provisioned at ".../zpico-sys/zenoh-pico". Run: nros setup
--source zenoh-pico (or git submodule update --init packages/rmw/zenoh/zpico-sys/zenoh-pico). -- #0390
```

The batch path wrote an `.unprobeable` marker for each of the four island
nodes, keyed on `probe_inputs_key` (the component's sources, the CLI stamp,
the probe's crate closure). `nros setup --source zenoh-pico` was then run, and
the board build's sync said, for every node:

```
sync: source metadata -- no producer for autoware_mrm_handler::mrm_handler (probe failed at this source
last sync; unchanged; last error: error: failed to run custom build command for `zpi...
```

Provisioning a source changes nothing the key covers, so the marker matched
and the probe was never run again. The bake fell back to the descriptor
without probe sidecars. `nros-node`'s build then refused every subscription's
`endpoint.registration_path` (28 warnings) and priced each at the closure
buffer (1,496 B) instead of no receive region (the island's backend
dispatches in place). Over the island's tiers that is the 31,416 B.

Those warnings end with "Run `nros sync` in the workspace so its probe
sidecars are current". With the marker in place, `nros sync` could not do
that. The only way out was to delete the markers by hand, which is how issue
1469 ended too.

The code's own rule, written for 1578, is "a NEGATIVE marker keyed on less
than what could fix the probe hides a repair". An unprovisioned source is
exactly that case: the repair is the host's store, which no key covers.

## Resolution

`mark_unprobeable_with_reason`, the one place all three recording sites go
through (the C/C++ batch, the whole-project configure failure, and the
deploy-bound Rust branch), writes no marker when the failure names an
unprovisioned source. It also clears an older marker. Every "not provisioned"
refusal in the tree gives the same remedy text, `nros setup --source <name>`
(the zenoh-pico and mbedTLS build scripts, `nros sdk-path`, the Rust probe's
#0390 remedy), so the check looks for that text rather than a list of source
paths. If it matches by mistake, the cost is one re-probe per sync. It can
never produce a wrong sidecar.

The next sync after `nros setup --source` probes again. Until then, each sync
fails fast and prints the remedy, instead of reporting a cached "unchanged".

Test: `an_unprovisioned_source_failure_leaves_no_marker` in
`packages/cli/nros-cli-core/src/orchestration/metadata_refresh.rs`, which
uses the island's real failure text. It fails without the fix, because the
marker is honoured at the same key. A compile error still records a marker
(negative control).

On the island: with the source provisioned and the probes run, the image is
back to 33,176 B of level backing, and the board links at the DTCM it had
before.
