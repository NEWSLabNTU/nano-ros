---
id: 1129
title: "58 more sites launder a fixture-resolver `Err` into a `skip!`, and 54 of them use a spelling `check-skip-budget` cannot see"
status: resolved
area: testing
severity: medium
related: [0584, 1112, 1124, 0196, phase-450]
---

## What

Issue 1124 converted the twelve sites matching its grep — `Err(e) =>
nros_tests::skip!("… fixture not prebuilt ({e})")` in five named files. Running
the sweep afterwards showed the scope had been the SPELLING, not the defect:

```sh
grep -rn 'skip!' packages/testing/nros-tests/tests/ | grep -iE 'not prebuilt|not built'
```

* **4** further sites still say `not prebuilt`, in files 1124's table did not
  list: `native_api.rs:147`, `roundtrip_xprocess_e2e.rs:{519,522,525}`.
* **54** say `not built`, across **43 files** — `entry_e2e`, `params`,
  `safety_e2e`, `native_example_pubsub_e2e`, `native_example_reqresp_e2e`,
  `multihost_e2e`, `roundtrip_xprocess_e2e`, `interop_e2e`, `qos_override_e2e`,
  `qos_overrides_runtime_delivery`, `realtime_subnode_cpp_e2e`,
  `executor_sizing_e2e`, `deployed_native_system_e2e`,
  `contract_monitor_parity`, `c_riscv_nuttx_e2e`, … — almost all shaped
  `.unwrap_or_else(|e| nros_tests::skip!("… fixture not built: {e}"))`.

Same laundering as 1124: issue 0584 makes an absent IN-LANE fixture a hard
failure, and out-of-lane never reaches these arms at all (`require_in_lane` /
`require_coord_in_lane` / `require_workspace_in_lane` panic `[SKIPPED:lane]`
inside the resolver, before the existence check).

## Why the `not built` half is worse than the twelve

`scripts/test/check-skip-budget.py` matches on

```python
FIXTURE_RE = re.compile(r"not prebuilt|fixture binary MISSING", re.I)
```

**`not built` does not match that.** So the twelve 1124 fixed were at least loud
at the gate — a red twenty minutes downstream that could not name its site,
which is exactly what 1112 described. These 54 launder the `Err` into a skip AND
evade the gate written to catch that laundering. The gate reads as covering the
class while covering under a fifth of it: the 0196 shape, on the gate rather
than on the rule.

Two independent fixes are wanted and they are not substitutes:

1. **Widen `FIXTURE_RE`** so the gate's coverage matches its stated rule — it is
   one regex, and it is the half that cannot rot. Expect it to go red on a lane
   immediately; that red is the finding.
2. **Convert the sites**, per-site and with evidence, the way 1124 did.

## Why it is not a blind sweep

1124's own reasoning applies unchanged, and here the answer really is not
uniform:

* Some of these arms sit behind `NROS_FIXTURES_OPTIONAL` (the documented
  light-tier opt-out, honoured one frame up in the resolver) — those are correct
  skips and must stay.
* Some are west/SDK-preconditioned (`qos_zephyr_ros2_interop_e2e`,
  `zephyr_leaf_staleness`) where "not built here" is a host fact, not a lane gap.
* The rest want the 1124 method: which lane(s) run the test, does the fixture
  have a manifest row in that lane's build set, then `.expect(...)` or a comment
  naming the lane.

## Method

Exactly issue 1124's, which is worked end to end in that issue:

1. `cargo run -p nros-tests --bin lane-coords -- <lane>` for each lane's
   coordinate set.
2. `python3 scripts/build/fixtures-manifest.py {coords,list,list-workspaces}
   [--coords-from FILE]` for the row's coordinate and each lane's build set.
3. In lane ⇒ `.expect(...)` (and add the row to the lane's build if missing);
   out of lane ⇒ keep the skip and comment which lane, so it is not
   re-litigated.

Doing (1) — the regex — first is worth it: it turns the sweep from a reading
exercise into a lane that tells you which sites actually fire.

## Resolved (phase-450 W1, 2026-09-11)

Both halves, in the order the issue asked for.

**The gate.** `FIXTURE_RE` read `not prebuilt` only; 57 of the tree's 61
laundering sites say `not built`, so the rule was stated fully and enforced on
4 of 61. Widened, with six `--self-test` cases (four positive, two that must NOT
match) and mutation-tested by reverting the regex.

**A structural finding the conversion survey turned up, which had to land
first.** `require_prebuilt_workspace_binary` carried NONE of the tier block its
sibling has — no `.build-failed` marker check, no `NROS_FIXTURES_OPTIONAL` skip,
no `gate_promised_fixtures()` panic. 21 of the 61 sites resolve through it, so
for those the call-site `skip!` was not laundering a verdict, it was the only
accommodation that had ever existed — and on a gated run the only thing between
CI and a silent green. One shared helper (`absent_fixture_verdict`) now, called
by both, with the per-resolver remedy threaded through.

**The sites.** 51 converted, 10 kept, 0 unsure. The keeps are not fixture
resolvers at all: `launch_resolver_bin` is a host tool (6 sites),
`zephyr_leaf_staleness` probes a west dir with a bare `is_file()` (2), and
`zenoh_header_parity` / `zenoh_archive_symbols` already guard on the opt-out and
panic in the else.

One override of this issue's prose, on the evidence:
`qos_zephyr_ros2_interop_e2e` is listed here as a west/SDK keep, and it goes
through `require_prebuilt_binary_fresh_zephyr`, which resolves both the west lane
and the opt-out. Converted.

Three reasons named the wrong binary ("native listener fixture" while resolving
`build_int32_sink`) and now name `packages/testing/nros-tests/bins/int32-sink`.
