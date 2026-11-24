---
id: 2926
title: "Adding a QoS contract to a C or C++ image grows its executor arena by ~28 KB for two action endpoints the arena already held"
status: open
type: bug
area: [sizing, cmake, memory, book]
severity: medium
related: [2927, 2768, 2425]
found: 2026-11-24
---

## What happens

The tutorial's "tell the build what you need" chapter adds a
`system.contract.yaml` to the sensor workspace (two `depth: 1` rows) and says
static RAM falls. On the C and C++ tracks it RISES by ~36 KB of `bss`. Most of
that is the executor arena.

Measured on `main` at `cdb14ab4ab`. The C track of the book replayed to the
end of that chapter (installed track, qemu variant, `mps2-an385-freertos`,
release images). `nros-mem-report.py --baseline`, before vs after the
contract:

| | before | after | delta |
| --- | --- | --- | --- |
| `bss` | 1,221,512 | 1,257,312 | +35,800 |
| `__nros_executor_storage` | 83,680 | 111,952 | +28,272 |
| `SMALL_PAYLOADS` | 12,288 | 16,384 | +4,096 |
| `static_subscriber_storage::SLOTS` | 3,072 | 4,096 | +1,024 |
| `SERVICE_BUFFERS` | 14,256 | 15,228 | +972 |
| `g_sessions` | 89,448 | 90,112 | +664 |

The C++ track has the same shape (+36,032 `bss`, from the book's own probe run of all three tracks). The Rust track does
not: +2,336 `bss`, of which +3,072 is issue 2927.

The configure lines say where the arena term comes from:

```text
before: entity inventory DERIVED from 6 components -- 18 entities, 15 executor callback slots -> NROS_EXECUTOR_MAX_CBS, 0 of them action-sized -> NROS_EXECUTOR_ACTION_CLIENTS
after:  entity inventory DERIVED from 6 components -- 20 entities, 17 executor callback slots -> NROS_EXECUTOR_MAX_CBS, 2 of them action-sized -> NROS_EXECUTOR_ACTION_CLIENTS
```

The sizing descriptor agrees:
- **Before** (`basis = "probe"`): the action server and client appear only as
  their constituent topics (`/fibonacci/_action/status`, `/feedback`, …), and
  `action_*_entities = 0`.
- **After** (`basis = "contract"`): it lists `action_server` and
  `action_client` rows, and `action_client_slots = 2`.

## Why it is wrong, measured

Both images were booted in QEMU against a router. The workspace runs one
action server and one action client:

| | arena capacity | claimed at first spin | action round trip |
| --- | --- | --- | --- |
| before | 40,200 | 18,392 | goal accepted, `result last=55` |
| after | 68,048 | 18,216 | goal accepted, `result last=55` |

The C image without the contract already registers and runs both action
endpoints in an arena of 40,200 bytes, claiming 18,392. The contract adds
27,848 bytes of capacity and the image claims no more of it. So the two
action-sized slots the contract road derives are not bytes a C or C++ action
client takes from this arena. At least on this road, it is an over-size, not
a correction of an under-size.

## Not yet established

- **Which half is off.** Either the action-sized slot price (the Rust
  `ActionClient`, which does live in the arena) is applied to C/C++ action
  entities whose storage is elsewhere, or the census road under-reports
  actions and the arena merely had headroom. The equal claim on both boots
  points at the first.
- **The other +6 KB.** One more subscriber slot (`SMALL_PAYLOADS`) and the
  queryable and liveliness counts (`service_server_queryables` 4 → 7,
  `entity_liveliness_tokens` 14 → 23) move with the action rows too. Each
  needs the same check against what the image registers. `SLOTS` +1,024 may
  instead be issue 2927: the `/sensor/reading` subscription was
  `typed_bound` before the contract and is refused after.

## What would close it

A C or C++ image's arena is sized for the action endpoints it registers, the
same with and without a contract. The tutorial's C/C++ "after" image is then
no larger than its "before" for the endpoints the contract does not mention.
Acceptance is a boot of both images reading the same claim, and the
chapter's numbers re-measured.

## Found with

The tutorial chapter's claim (`book/src/tutorial/tell-the-build.md`: 219,360 B
saved) broke in all three tracks. The Rust half is mostly not a defect: issue
2768 sizes the parameter store from declarations without a contract, so that
saving now lands in the "before" image. Issue 2927 is the rest of the Rust
half.
