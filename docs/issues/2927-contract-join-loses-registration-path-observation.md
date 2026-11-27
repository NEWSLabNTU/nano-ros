---
id: 2927
title: "Once a contract names a subscription by its resolved name, the probe's registration observation no longer joins, so the row is priced as buffered"
status: open
type: bug
area: [sizing, cli, orchestration, memory]
severity: low
related: [2926, 2425, 1594]
found: 2026-11-24
---

## What happens

The tutorial's sensor workspace launches `sensor_node` in the `/sensor`
namespace, and its subscription is written `reading`. The workspace's
sizing descriptor states that subscription's `registration_path` from the
metadata probe until a `system.contract.yaml` is added. After that, it
refuses the field.

Measured on `main` at `cdb14ab4ab`, tutorial replay (installed track, qemu,
`mps2-an385-freertos`, release), sizing descriptor before and after the
chapter's contract:

| track | before (`basis = "probe"`) | after (`basis = "contract"`) |
| --- | --- | --- |
| Rust | topic `/reading`, `registration_path = "in_place"` | topic `/sensor/reading`, `registration_path` refused |
| C | topic `/reading`, `registration_path = "typed_bound"` | topic `/sensor/reading`, `registration_path` refused |

The refusal reads: "no per-endpoint observation was joined onto this row …
none that is CURRENT could be attributed to this endpoint by the contract join
(no workspace named to this producer, a stale or missing sidecar, a remapped
node, a relative written name, or a key that is not unique on both sides)".

A refused row keeps its receive region, which is RFC-0100's safe direction.
The cost on the Rust image is `EXECUTOR_BACKING` +3,072 B (`nros-mem-report.py
--baseline`). That more than cancels the contract's own savings there
(transient-local slots −1,344, service buffers −324, `data` −1,776). So the
tutorial's Rust "after" image is 560 bytes larger than its "before".

## What is known

Both descriptors come from the same workspace, the same build and the same
probe sidecars. Only the contract was added. So the observation exists; what
changes is the key the row is looked up by. Without a contract the row
carries the WRITTEN name (`/reading`, the namespace not applied), and with
one it carries the RESOLVED name the contract states (`/sensor/reading`).
The contract's QoS attributes onto the row (it gains `depth = 1`), so the
join of the contract itself succeeds. It is the sidecar's observation that no
longer attributes.

Not yet established: whether the sidecar join keys on the written name while
the contract join resolves the namespace, or the reverse. The refusal lists
five causes and does not say which one fired, which is itself worth fixing.

## What would close it

The same endpoint states the same `registration_path` with and without a
contract when its node is namespaced and its name relative (the tutorial's
shape). The refusal, where one remains, names the cause that fired.
Acceptance: the tutorial's Rust "after" image has `EXECUTOR_BACKING` equal to
the "before" image's, less what the contract's depths save.

## Found with

Issue 2926, the same replay. The tutorial chapter's RAM claim broke in all
three tracks.
