---
id: 1808
title: "Seven sizing knobs are still set by hand because nothing in the image can derive them"
status: resolved
type: limitation
area: [sizing, zephyr, memory]
severity: low
found: 2026-10-11
resolved: 2026-10-26
related: [1036, 1120, 1140, 1010, 1702, 1722, 1728, 2007, phase-394, phase-400, phase-516]
---

**Home: [phase-516](../../roadmap/phase-516-low-severity-issues-with-no-home.md) W4** since 2026-10-25. Before that, no home. Re-homed 2026-10-23 from phase-394 to [phase-400](../../roadmap/archived/phase-400-unified-configuration.md), which was archived the same day. [Issue 2007](2007-sizing-knob-ladder-rungs-backlog.md), which carried phase-400 W6's remainder, closed on the same day. Its two XRCE service buffers, the only rows that wait on a derivation, moved into the table below.

## Measured

[phase-412](../../roadmap/archived/phase-412-derived-counts-and-sizes.md) derived
every pool count and size the entity inventory can answer. On the safety
island (mr-canhubk344) its board `.conf` went from 324,096 B (98.91 % of 320 KB)
hand-set to 280,536 B (85.61 %) fully derived. All four acceptance items were
met on 2026-09-06, and `NROS_MAX_LIVELINESS` and the executor arena were derived
after that. The phase was archived on 2026-10-11.

What it could not derive is its W2 table. Each knob is still stated by the
operator, and each has a different blocker:

| knob | island value | blocker |
| --- | ---: | --- |
| `CONFIG_MAIN_STACK_SIZE` | 16384 | Needs frame analysis, not an inventory. phase-409 has the method (`objdump`, summing every `sub sp`) for one call chain; nothing turns it into a knob. |
| `NROS_ZEPHYR_HEAP_SIZE` | 94208 | Runtime allocation, no static model. Two partial answers have landed since phase-412. The default follows the declared capability: 512 KiB with `param_services`, 192 KiB with `lifecycle` (issue 1702). The boot record carries the high-water mark at first spin, and `read-boot-report.py` names the knob value (peak + floor - 512) it implies (issue 1728). A stated value below the derivation warns at configure. Exhaustion is loud (issue 0968; 1010 in the field). The remaining step is turning a measured peak into a stated value without a hand copy. |
| `NROS_GRAPH_CACHE_SIZE` | 4096 | Sized by the peer graph, which is not a property of this image. It is now stated as an entity count, `NROS_GRAPH_MAX_ENTITIES`, rather than as bytes (8f6fa8ee83). The cache compiles out entirely when every zenoh link is serial or CAN (`ZPICO_GRAPH_DISCOVERY`, phase-473 W1). Otherwise it stays operator-stated, likely permanently. **Closed as permanently operator-stated (2026-10-12):** `NROS_GRAPH_MAX_ENTITIES`'s help text records it as a declared capability, with the reason. |
| `NROS_ZEPHYR_TASK_SLOTS` | 5 | Transport tasks, not entities: the count of tasks ALIVE at once. Slots are released on join since issue 0839 (2026-08-27), so the count no longer grows with reconnects and a static answer exists. The Kconfig help said otherwise until 2026-10-12. Derivable from the transport's own declaration (a zenoh session runs read, lease and tx-flush tasks), but nothing declares it, and a derivation needs a Zephyr build to measure against. |
| `NROS_ZEPHYR_TASK_STACK_SIZE` | 8192 | Same as above. |
| `NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE` | n/a | Moved from issue 2007. One size is shared by every XRCE service-server slot: user services, action servers, and the runtime's parameter and lifecycle services. Issue 1722 RAISES it to the priced `set_parameters` request (`XRCE_PARAM_REQUEST_BYTES`). Deriving it DOWN to the declared servers' `_Request` bound would under-size lifecycle's `ChangeState`, whose `label` is an unbounded string. Zenoh derives its slot (`NROS_SERVICE_INBOX_BYTES`) because phase-461 W2 moved the parameter and lifecycle families onto a ring of their own. XRCE needs the same split first. |
| `NROS_XRCE_SERVICE_REPLY_BUFFER_SIZE` | n/a | Moved from issue 2007. A client receives the `_Response`, and the sizing descriptor has no response bound: `received_types_of` in `nros-cli-core/src/sizing_descriptor.rs` prices a `ServiceClient` row by its `_Request`, like a server row. Not derivable until the descriptor states a response bound. Because zenoh's `SERVICE_FAMILY` includes client rows, a client's request bound also enters zenoh's server inbox maximum. That over-sizes the inbox; it never under-sizes it. |

The island's board `.conf` carries each blocker as a comment beside the value.

## Related work that is not this issue

- Which sizing knobs have ladder RUNGS is a different question.
  [Issue 2007](2007-sizing-knob-ladder-rungs-backlog.md) closed it:
  a rung is added when a descriptor needs a non-default value, and
  `python3 scripts/check/config-knob-census.py` lists the 14 that have none.
- Two issues phase-412 homed stand on their own: 1120 (the inventory's refusal
  names the retired `ENTITIES`) and 1140 (two "was a contract authored?"
  predicates disagree on a timer-only contract). A third, 1036, is resolved.

## Closes when

Each row above is either derived from something the image declares, or recorded
as permanently operator-stated with its reason in the knob's own help text. The
graph cache is likely the second kind.

## Resolution

Each of the six remaining rows is now derived or ruled permanently
operator-stated, with the reason in the knob's own help text.

| knob | outcome |
| --- | --- |
| `NROS_ZEPHYR_TASK_SLOTS` | **Derived** in Kconfig from the transport's own tasks: a zenoh session runs read and lease, plus tx-flush with `NROS_ZENOH_TX_BATCH`, and the Zephyr road builds zenoh-pico with one session. So 3 with batching, 2 without; other backends keep 4. Measured on native_sim, the C action server: at 2 slots it prints `OUT OF THREAD SLOTS` and `zpico: tx-flush task init FAILED`; at 3 it boots clean, and `example_e2e::case_08_zenoh_c_action_e2e` passes. The default was 4; the island states its own 5 in its board conf, which still wins. |
| `NROS_XRCE_SERVICE_REPLY_BUFFER_SIZE` | **Derived.** The sizing descriptor gained `[[endpoint]] reply_bound_bytes` for client rows (`_Response`, or the largest of an action client's three responses; additive, no schema bump), and `nros-rmw-xrce-cffi/build.rs` sizes the slot from it plus the CDR header. A reply above `XRCE_BUFFER_SIZE` now raises the slot rather than being dropped. |
| `NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE` | **Derived**, after the split the row asked for. XRCE now gives the parameter and lifecycle services slots of their own (`XRCE_BUILTIN_SERVICE_SERVERS`, derived beside the total), chosen by name with zenoh's phase-461 W2b rule. Those slots keep `XRCE_BUFFER_SIZE` raised to the parameter request, so lifecycle's unbounded `ChangeState` keeps its floor. The application's slots are sized from its own declared servers. The derivation abstains, and says so, when a server row is unpriced or the rows do not account for every application server. Checked on the features `linux_rust_params_xrce` image (11 builtin slots, a `#error` probe confirmed the split compiled in): `xrce_set_parameters_naming_25_declared_lands` passes. |
| `NROS_ZEPHYR_HEAP_SIZE` | **Permanently operator-stated** beyond its per-capability defaults: the peak is a runtime property that moves with what the peers send. The help text says so and names `read-boot-report.py --heap-headroom` as the measurement. |
| `NROS_ZEPHYR_TASK_STACK_SIZE`, `CONFIG_MAIN_STACK_SIZE` | **Permanently operator-stated**: a stack's depth needs frame analysis of the built image, not an inventory. The help text names `CONFIG_THREAD_ANALYZER` and the guard region as the measurement. `CONFIG_MAIN_STACK_SIZE` is Zephyr's own symbol, so its ruling lives in the task-stack help beside it. |
| `NROS_GRAPH_CACHE_SIZE` | Already closed on 2026-10-12 (above). |
