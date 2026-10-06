---
id: 1687
title: "A workspace publisher advertises KEEP_LAST(1) / TRANSIENT_LOCAL whatever its
  code or its launch plan declares — C, C++ and mixed QoS cells and the
  qos_overrides cell all red"
status: resolved
type: bug
area: [rmw, codegen, testing]
severity: high
found: 2026-10-05
related: [1651, 1684, 1709, phase-480]
---

## Failing tests

- `nros-tests::workspace_features_e2e workspace_features::case_08_c_qos`
- `nros-tests::workspace_features_e2e workspace_features::case_13_cpp_qos`
- `nros-tests::workspace_features_e2e workspace_features::case_17_mixed_qos`
- `nros-tests::qos_override_e2e a_ros2_peer_sees_the_overridden_publisher_profile`

## Evidence

The three `workspace_features` cells declare `KEEP_LAST(10)` in code; the
`qos_override` cell declares `qos_overrides./qos_chatter.publisher.reliability
= best_effort` in the plan. `ros2 topic info -v` reports the SAME profile for
every one of them:

    Reliability: RELIABLE
    History (Depth): KEEP_LAST (1)
    Durability: TRANSIENT_LOCAL

(the matching `qos_listener` advertises `KEEP_LAST (4)`, so subscriptions are
not uniformly flattened). Neither the code-declared profile nor the plan
override reaches the advertised entity.

- CI: workflow_dispatch run 37252649866, tree of commit
  dc5691a584cfb9f2a53b4b92372370d81ee8cd38.
- Local, SOLO (`-j1`), fixtures built from that tree 2026-10-05 02:43–03:24
  UTC by `just build-test-fixtures lane=tier1`: all four red again, so not load
  and not a stale fixture.

## Where to look

`KEEP_LAST(1)` + `TRANSIENT_LOCAL` on a publisher is what a declared-QoS
contract row (phase-454 W12, `system.contract.yaml`) or a sizing-derived
default would impose, so suspect a layer that now overrides the per-entity
profile after the node sets it. Not bisected — no lane ran these between
2026-06-17 and now (issue 1651).

## Acceptance

All four pass solo and in the tier-1 `test-all`.

## Resolution — 2026-10-06 (phase-480 W1)

**Nothing dropped the profile.** Two different things were on the wire, and
neither was "a layer overriding the per-entity profile":

1. **Depth: the backend's real capacity, advertised honestly.** Run by hand,
   each entry logs its grant, e.g.

       qos: publisher '/qos_chatter' asked for TRANSIENT_LOCAL KEEP_LAST(10);
       this backend retains 1 sample … Granting 1 and advertising it to the graph.
       qos: subscription '/qos_chatter' asked for KEEP_LAST(10); this image's
       receive ring holds 4. Granting 4 …

   Since phase-428 W9 / phase-455 W5 the zenoh shim puts the GRANT into the
   liveliness token, and a transient-local publisher retains
   `TL_RETAIN_DEPTH` = 1 sample. `TRANSIENT_LOCAL` on the wire is proof the
   code's profile arrived (the default is VOLATILE). The cells were written
   before W5 (2026-08/09 vs 2026-09-13) and asserted KEEP_LAST(10) — which is
   also the DEFAULT depth, so that assertion never told a declared entity from
   a defaulted one.
2. **Reliability: a real defect in the shim.** `qos_override_e2e`'s
   `best_effort` override reached the backend (`asked for BEST_EFFORT` in the
   log), and `shim/qos.rs::admit` rewrote it to RELIABLE on the premise that
   `zpico.c` "sets `Z_CONGESTION_CONTROL_BLOCK` unconditionally". It does not:
   BLOCK is set only under the opt-in `ZPICO_TX_BATCH`; every other build
   publishes with zenoh-pico's default, DROP. BEST_EFFORT is served by any
   delivery, so it is now granted as asked.

**Fix** (`fix/1687-declared-qos-on-wire`):

- `shim/qos.rs`: BEST_EFFORT granted as asked on every entity kind; only an
  unstated policy still resolves to RELIABLE. Docs in `session.rs` and
  `publisher.rs` corrected.
- The four demo pairs (`c`, `cpp`, `mixed`, `rust` QoS talkers and listeners
  in `examples/workspaces/features`) declare RELIABLE + TRANSIENT_LOCAL +
  KEEP_LAST(1) — the profile this backend serves exactly, distinct from the
  default in two policies. The Rust talker no longer claims a late joiner gets
  "the last 10 samples".
- The QoS cells assert `KEEP_LAST (1)\n` (the newline keeps `(10)` from
  matching); `qos_override_e2e` asserts BEST_EFFORT + TRANSIENT_LOCAL +
  KEEP_LAST (1), and its module doc no longer says the backend has no QoS
  semantics.

**Before / after** (fixtures rebuilt from each tree, solo):

- before (origin/main 52dcf5b5a2): all four red, `ros2 topic info -v` showing
  RELIABLE / KEEP_LAST (1) / TRANSIENT_LOCAL for the publisher and
  KEEP_LAST (4) for the listener.
- after (rebased onto 3a4bf2e191): `case_08_c_qos` PASS 5.1 s,
  `case_13_cpp_qos` PASS 5.0 s, `case_17_mixed_qos` PASS 5.1 s,
  `qos_override_e2e::a_ros2_peer_sees_the_overridden_publisher_profile` PASS
  1.6 s.
- negative control: the rust-qos fixture rebuilt with the old `qos.rs` fails
  `qos_override_e2e` with `Reliability: RELIABLE`.
- unit: `best_effort_is_granted_as_asked_on_every_kind`; all 102
  `nros-rmw-zenoh` lib tests pass; `check-qos-mask-derivation` OK.

Sweep: `grep -rn 'granted.reliability\|granted RELIABLE\|GRANTS reliable'
packages --include='*.rs'` (no other backend rewrites the grant) and
`grep -rln 'transient_local()\|TRANSIENT_LOCAL\|TransientLocal' examples`
(the only other hit, the PX4 offboard companion, is XRCE-only).

**Not measured / left behind:**

- A deeper retained history is a backend extension, filed as issue 1709
  ([open](../1709-zenoh-tl-retain-depth-is-a-constant-one.md)).
- The Zephyr `zephyr_rust_qos` image shares `rust_qos_talker_pkg`, so its
  declared depth is now 1 too. It was already GRANTED 1 there; the Zephyr cell
  was not rebuilt or run.
- Under `ZPICO_TX_BATCH=1` (BLOCK) a BEST_EFFORT publisher now advertises
  BEST_EFFORT while delivering reliably — an over-delivery that BEST_EFFORT
  permits. Not run.
- Whether a RELIABLE writer under DROP congestion control truly honours
  RELIABLE is the inverse question and is not addressed here.
- `qos_overrides_runtime_delivery` was not re-run (its fixture is in the Rust
  zenoh lane, not rebuilt on the rebased tree); it asserts the REQUEST logged
  by the node, not the shim grant.
