# Phase 473 -- zenoh on a constrained link: graph discovery off, latched reads on

**Status (2026-09-29). W1 in review (#1435); W2 LANDED (#1437).** Two backend changes the
safety-island demo (simple-autoware-safety-island, phase 8, design D9 and gaps
G3/G4) needs from `nros-rmw-zenoh`, each measured on the island's QEMU image
(mps2/an385, LAN9118, 1 MiB heap) against a stock `rmw_zenohd` and
`rmw_zenoh_cpp` 0.1.9 host nodes.

**Prior:** phase-428 W13 (the standing liveliness subscriber and the graph
cache), phase-455 W5 / issue 1341 (transient-local served by a PUBLISHER; the
subscriber half refused by design), phase-412 (the knob ladder and
`check-knob-delivery`).

---

## Why

The island joins a host domain that runs Autoware: about 136 nodes. Two
failures were measured on the island before this phase (its
`docs/serial-link.md` and `demo/l3/README.md`):

1. **The graph burst.** The first service client a node creates starts the
   graph cache: a liveliness subscriber with history on `@ros2_lv/<d>/**`. The
   router answers it with every token of every node on the domain, at once.
   Over a 921,600-baud UART 2.5 KB of that history in three back-to-back frames
   overran the receive ring and stopped the read task; over TCP the QEMU image
   died of heap exhaustion 2.3 s after joining Autoware's graph, with a 120 KiB
   heap and with a 1 MiB one. The only stopgap was a router ACL denying
   liveliness tokens toward the island.
2. **The late join.** `/api/operation_mode/state` is published on CHANGE by
   `default_adapi`, transient-local. The island's reader had to be VOLATILE
   (the backend refused a transient-local subscription), so an island that
   booted after the mode was set read UNKNOWN, started an MRM at once, and
   oscillated NORMAL/MRM_OPERATING 21 times in one run.

The island makes no graph query of its own, and a service call is a `z_get`
on the service key expression whatever the cache holds. So on such a link the
cache costs the link and buys nothing.

## W1 -- `NROS_ZENOH_GRAPH_DISCOVERY` (#1435)

A bool knob, `ZPICO_GRAPH_DISCOVERY` (env/cargo) and
`CONFIG_NROS_ZENOH_GRAPH_DISCOVERY` (Kconfig), on the usual ladder:

| rung | where |
| --- | --- |
| environment | `ZPICO_GRAPH_DISCOVERY=0/1`, read by `_nros_resolve_knob` (Zephyr) and `nros_zephyr_build::knob` (cargo) |
| Kconfig / board | `CONFIG_NROS_ZENOH_GRAPH_DISCOVERY`, paired in `KCONFIG_PAIRS` |
| derived | off when every zenoh link the image compiles is serial or CAN (ISO-TP included); on otherwise. Kconfig `default n if ...`, `graph_discovery_derived` (nros-zpico-build), and the `zenoh.graph_discovery` implication of `transport.kind=serial` (nros-platform-config) |
| default | 1, which `zpico.c` also `#define`s |

With it off, `zpico.c` compiles out the cache (`graph_cache_t`, its mutex and
its subscriber) and `zpico_graph_cache_start` answers `ZPICO_ERR_CONFIG`. The
session latches that answer (`graph_discovery_off`), says so once at INFO, and
every graph query answers `Unsupported`. The image still DECLARES its own node
and entity tokens, so a host's `ros2 node list` sees it as before. A service
client is unchanged except that `service_is_ready` can no longer say yes;
the call itself is a `z_get` the router matches against the queryables it
knows.

The define is always delivered as 0 or 1 (`zpico.c` has a fallback of 1, so a
missing define would compile the cache back in silently), guarded by an
`#error` for any other value and by `check-knob-delivery`.

### Provenance lines, measured

The three rungs, from the island's `qemu-build` configure step (the em dash cmake prints is written `--` here):

```
-- nros: ZPICO_GRAPH_DISCOVERY=1 DERIVED from this image's zenoh links (tcp); nothing in Kconfig or the environment states otherwise
-- nros: ZPICO_GRAPH_DISCOVERY=0 DERIVED from this image's zenoh links (serial); nothing in Kconfig or the environment states otherwise
-- nros: ZPICO_GRAPH_DISCOVERY=0 from environment (Kconfig says 1) -- environment wins
```

### Measured on the island (QEMU, TCP, 1 MiB heap)

Router: stock `rmw_zenohd`, `RUST_LOG=zenoh=debug`. Before the island boots,
100 `rclpy` nodes on `rmw_zenoh_cpp` (3 publishers, 2 subscriptions and 1
service each) are up. A TCP tap between the island and the router counts
`@ros2_lv` keyexprs per direction; `ros2 node list` runs 30 s after boot;
the heap numbers are the image's boot report, read from a memory dump 3 s
before the 60 s bound.

Discovery ON (the derived default on TCP):

```
bytes island->router 12294, router->island 362957
@ros2_lv occurrences island->router 35 (last at +7.771 s)
@ros2_lv occurrences router->island 1500
router->island @ros2_lv after the island's last declaration: 1500 occurrences in 1053 chunks
synthetic node names seen router->island: 100
  platform heap PEAK            964464 bytes   (91.9% of the heap)
nros: HEAP EXHAUSTED: request 197 bytes, arena 1049088 bytes, caller 0xec23
nros: PANIC platform heap exhausted (see the HEAP EXHAUSTED line above, and the boot report's failed_alloc_size)
[00:00:02.534,000] <err> os: >>> ZEPHYR FATAL ERROR 4: Kernel panic on CPU 0
```

Discovery OFF (`ZPICO_GRAPH_DISCOVERY=0`), same graph:

```
== ros2 node list t=+30 s ==
/mrm_comfortable_stop_operator
/mrm_emergency_stop_operator
/mrm_handler
/stop_mode_operator
(synthetic nodes listed: 100)
bytes island->router 79645, router->island 175
@ros2_lv occurrences island->router 34 (last at +7.769 s)
@ros2_lv occurrences router->island 0
router->island @ros2_lv after the island's last declaration: 0 occurrences in 0 chunks
synthetic node names seen router->island: 0
stage      6  FirstSpin -- registration complete and spinning
  platform heap PEAK            105368 bytes   (10.0% of the heap)
```

Static RAM: the zpico session pool (`g_sessions`) is 20,000 bytes with
discovery off and 24,136 with it on, the 4,096-byte cache plus its bookkeeping.

A service call with discovery off: the island's `mrm_handler` called
`/system/mrm/emergency_stop/operate` on a host-side `rclpy` server
(`OperateMrm`), which logged `SRV_CALL /system/mrm/emergency_stop/operate
operate=True`. No graph lookup is on that path.

### Acceptance

- [x] The knob exists on every rung, with a provenance line for each (above).
- [x] Off: no liveliness subscriber, no cache, no `@ros2_lv` toward the image
      after its own declarations; the host still lists the image's nodes.
- [x] Off: the heap does not grow with the host graph (105,368 bytes peak
      against 100 foreign nodes; ON exhausts 1 MiB in 2.5 s).
- [x] Off: a service call works against a host server.
- [x] `check-knob-delivery` holds the define to the resolved value.

## W2 -- the subscriber half of transient-local (LANDED, #1437)

At subscription creation, when the granted durability is TRANSIENT_LOCAL, the
shim issues ONE history query on `<subscription keyexpr>/@adv/**`, the global
query `rmw_zenoh_cpp` 0.1.9's advanced subscriber issues, which intersects every
advanced publisher's cache queryable at `<topic>/@adv/pub/<zid>/<eid>/_` (a
stock one, and nano-ros's own since phase-455 W5). The replies go through the
same ring producer as live samples. While the query is open a sample from the
same publisher (GID in the rmw attachment) that is not newer than one already
delivered is dropped, so a cache reply that lands after a newer live sample
cannot leave the ring stale. `admit` then grants transient-local to a
subscription; a service or a client is still refused.

The query must accept replies on any key (`Z_REPLY_KEYEXPR_ANY`, the `_anyke`
parameter): the cache replies on the TOPIC key, which does not intersect the
query key, and without it the publisher's session sends only the final reply.
Measured before that line was added: the router routed the query to the
latched `rclpy` publisher and propagated its final reply 0.5 ms later, and the
island never saw the value.

Measured on the island (discovery off, as it runs): a host node publishes
`/api/operation_mode/state` ONCE, latched (AUTONOMOUS), then the island boots
and its `mrm_handler` subscribes transient-local. The host records the
island's `/system/fail_safe/mrm_state`:

Before `_anyke` (the value never arrived):

```
0.000	PUB_MODE_ONCE	AUTONOMOUS
5.814	MRM_STATE	NORMAL	first
5.877	SRV_CALL	/system/mrm/emergency_stop/operate	operate=True
5.878	MRM_STATE	MRM_OPERATING	change
```

With it (NORMAL for the rest of the 60 s run, no transition):

```
0.000	PUB_MODE_ONCE	AUTONOMOUS
6.183	MRM_STATE	NORMAL	first
```

Memory: `subscriber_entry_t` grows from offset 0x34 to 0x58 (36 bytes,
including padding) per entry, 396 bytes over the island's 11 entries. The
query itself holds one zenoh-pico pending query until its final reply or the
5 s timeout; the heap peak of the late-join run was 103,552 bytes, the same as
a control run with the mode republished at 10 Hz.

Not done: per-publisher late-joiner detection (the `@adv` liveliness token a
stock advanced subscriber also watches). A latched publisher that appears
AFTER the subscription delivers its first sample live.

### Acceptance

- [x] A transient-local subscription is granted and issues the history query.
- [x] A late-joining island reads a latched value published before it booted.
- [ ] Against the full Autoware container (the island demo's `l3-autoware`):
      not yet run.

## What this phase deliberately does not do

- It does not change what a host sees of the image: tokens are still declared.
- It does not filter the graph by prefix (a partial cache). Off is the answer
  for an image that makes no graph query; a partial cache is a separate design.
