---
id: 903
title: "`get_topic_names_and_types` returns EMPTY against a live rmw_zenoh_cpp
  node, while `get_node_names` on the same session returns the node"
status: open
type: bug
area: rmw
related: [phase-381, issue-0791]
---

## Measured

Live interop, ROS 2 Humble + `rmw_zenoh_cpp` in the repo's `ros2` distrobox, on
the box's own tree, domain 0, stock `demo_nodes_cpp talker` plus a
`rmw_zenohd` router. Probe: `packages/testing/nros-tests/bins/graph-probe`.

```
GRAPH_NODE /|talker
GRAPH_PROBE_NODE_COUNT  1
GRAPH_PROBE_TOPIC_COUNT 0
GRAPH_PROBE_SAW talker
probe_rc=0
```

`ros2 node list` on the same graph shows `/talker`, and `ros2 topic list` shows
`/chatter /parameter_events /rosout`. So the session is connected, the graph is
populated, and node enumeration WORKS — the topic form returns nothing.

Both forms poll to convergence with a 20 s budget, so this is not the documented
warm-up. The probe was rebuilt from a cleared fingerprint (`Compiling
nros-rmw-zenoh`, `Compiling graph-probe`) to rule out a stale artifact, which had
masked one earlier attempt.

## What is already ruled out

* **The grammar.** Instrumenting the drain to dump every keyexpr before parsing
  showed real `rmw_zenoh_cpp` tokens arriving and **zero refused** — the 13-chunk
  entity shape phase-381 W2 pinned against our own builders matches the wire.
  Sample: `@ros2_lv/0/<zid>/0/5/SS/%/%/talker/%talker%set_parameters_atomically/
  rcl_interfaces::srv::dds_::SetParametersAtomically_/TypeHashNotSupported/
  ::,1000:,:,:,,`
* **The domain.** Same result on domain 0 and 71; the wildcard embeds the domain
  and both sides were matched.
* **The first drain defect.** `for_each_entity` restarted its query on
  `zpico_liveliness_get_check`, which reports the FIRST reply rather than the
  finished sweep, truncating every sweep to one poll window (2 tokens of a
  dozen). Fixed by `zpico_liveliness_collect_done`; that fix is what made
  `get_node_names` work.
* **Refresh-inside-the-drain.** `names_and_types` drains TWICE per reported name,
  and retiring a finished sweep at the end of each drain made the second pass
  restart what the first had used. Moved to a `refresh_query` called once per
  public entry point. Necessary, and did not fix this.

## Not established

Why the entity query yields nothing while the node query yields. The two use
separate standing queries and different wildcards — 9 chunks for nodes,
13 for entities — and only the node one has been observed working end to end.

The next measurement is the obvious one and was not run: dump the raw keyexprs
for the ENTITY query specifically, the way the node path was dumped. The
diagnostic used earlier lived in the box tree and was overwritten by a re-sync;
it should be an opt-in behind an env var in the committed source instead, so it
survives and so the next person does not re-derive it.

A plausible cause worth checking first: `names_and_types_filtered`'s outer loop
`break`s the moment a pass finds no new name, and a freshly started query always
returns nothing — so the loop can exit before the view has warmed, every call,
independently of how long the CALLER polls.

## Why this matters more than the count

Everything else in phase-381 was verified against our own builders, our own
parser and our own vtable: twelve slots produced, mutation-tested unit coverage,
three language surfaces, a clean `check-api-parity`. All of it passed while this
did not work. The slot existing, being produced, and being reachable is not the
same as the query answering — which is the exact overstatement issue 0800 exists
to name, reached this time through a path no unit test covers.

## Update 2026-08-30 — four defects fixed, the symptom MOVED, still open

Four independent defects were found and fixed by live interop against the same
stock talker. Each was real, and none of them is the whole bug.

1. **The drain restarted on the FIRST reply, not on a finished sweep.**
   `zpico_liveliness_get_check` returns 1 as soon as one reply lands, so a sweep
   was retired after one poll window and re-issued, and only whatever arrived in
   that window was ever seen. Added `zpico_liveliness_collect_done`, which
   reports the DROPPER firing.
2. **The refresh ran inside the drain**, so a caller could retire the sweep it
   was in the middle of reading. Moved to `refresh_query`, once per public entry.
3. **`CffiSession` dispatched only `get_node_names`.** The other four graph
   slots fell through to the not-supported arm, so the topic form could not have
   worked whatever the shim did. Covered now by
   `a_null_graph_slot_reports_unsupported_not_an_empty_graph`.
4. **`collect` was set AFTER the query was issued**, so replies racing the
   assignment were dropped. `zpico_liveliness_collect_start` is written longhand
   rather than delegating to `get_start`, so the flag precedes the get.

### The constraint underneath

**Two concurrent `z_liveliness_get`s do not both receive replies**, measured in
both directions:

* with the node sweep standing, the entity sweep got `arrived=0` across 99
  drains; with the node phase skipped, the same entity sweep got `arrived=2`
  immediately;
* once defect 3 was fixed and the entity sweep actually ran, node enumeration
  went from working to empty.

Collapsing both into ONE sweep on `@ros2_lv/<domain>/**` — which matches the
9-chunk node and 13-chunk entity shapes alike — is **worse, not better**:
measured, `**` delivered two tokens and no node token at all. That is recorded
in the code so it is not retried.

So the two sweeps are now SERIALIZED behind one slot, with a poll floor
(`GRAPH_QUERY_MAX_POLLS`) under `collect_done` so a sweep whose dropper never
fires cannot own the slot forever — which it did, deadlocking for 98 drains.

### Where it stands

```
GRAPH_TOPIC /parameter_events [rcl_interfaces::msg::dds_::ParameterEvent_]
GRAPH_PROBE_TOPIC_COUNT 1
GRAPH_PROBE_NODE_COUNT  0
```

**Topic enumeration works for the first time; node enumeration is now the empty
one.** The symptom moved rather than cleared, and the state is strictly better
than the two-slot form, which returned zero for BOTH once the topic path was
dispatched.

Two things remain unexplained and are the next measurements, not guesses:

* **Only one sweep ever starts.** `GRAPH_WILDCARD` prints once per run, and
  every `GRAPH_COUNTS` line is `["entities"]` — after the slot frees, the node
  sweep still never reaches the wire. Instrument the start path rather than
  reasoning about it.
* **Only 2–4 tokens arrive** where the talker declares roughly a dozen. Not the
  reply buffer: `ZPICO_GET_REPLY_BUF_SIZE` is 4096 and these keyexprs are ~140
  bytes. Not a timeout truncation either — the deadlocked sweep stayed open
  indefinitely and still saw two.
