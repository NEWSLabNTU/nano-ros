# phase-467 - board files state board facts: two executor knobs that must derive

**Status (2026-09-25). PROPOSED; W1 and W2 claimed the same day.**

Found by the Autoware Safety Island's phase 6 (`docs/roadmap/phase-6-l4-design-demo.md`
in `simple-autoware-safety-island`), which encoded the Reference Design WG's
L4 design as a contract and, on the way, audited the island's board file
against the derivation. Two knobs the contract fully determines are still
either hard-coded in source or defeated by a sentinel disagreement, and a
third (liveliness) was stated in the board file and had gone silently stale.
The third is the island's to fix (its phase-6 W7); the two here are nano-ros's.

## Why

The rule phase-412 set, and this phase restates: **a board file states board
facts (heap, stacks, task slots, the peer-sized graph cache) and never a count
the contract determines**, because a stated knob WINS over the derivation and
then drifts without a sound. The island proved the failure twice on
2026-09-24: `CONFIG_NROS_MAX_LIVELINESS=32` was correct at 29 tokens and
became 26 short the moment `params:` was declared (derived: 58 = 1 session +
4 names + 14 pubs + 11 subs + 2 servers + 2 clients + 24 parameter services),
and `CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES=1016`, hand-derived, is allocated
zero times because of the sentinel below.

## W1 - `MAX_MONITORS` gets the rung its siblings have

Issue 1471 (PR #1262) has the evidence: `pub const MAX_MONITORS: usize = 8`
at `packages/core/nros-node/src/executor/monitor.rs:202`, no knob anywhere,
while `MAX_CBS`, `MAX_SC`, `MAX_NODES` and the action-client count are all
generated in the SAME crate's `build.rs` (lines ~507, 515, 601, 642) from
`NROS_EXECUTOR_* / NROS_DECLARED_EXECUTOR_*` pairs. The island's C++ image
bakes 14 rate rows; `nros_cpp_install_monitors` refuses (`NROS_CPP_RET_FULL`,
-6) and the generated setup returns before creating a node. The image links
and would not boot.

What it does:

- Move `MAX_MONITORS` into `nros-node`'s generated config beside `MAX_CBS`,
  as TWO knobs: one for rate/latency rows (`monitor_rows`) and one for age
  rows (`age_rows`). One knob would price storage for a table the island
  does not have (its age table is 0 rows while its rate table is 14).
- Explicit rung `NROS_EXECUTOR_MAX_MONITORS` / `..._MAX_AGE_MONITORS`,
  derived rung `NROS_DECLARED_EXECUTOR_MAX_MONITORS` / `..._AGE_MONITORS`,
  produced by the entity inventory from the row counts the CLI already
  computes when it emits the entry. Crate default stays 8.
- The inline arrays `monitor_states` / `age_states` (`spin.rs:1554, 1565`)
  and every `.take(MAX_MONITORS)` site follow the generated consts.
- The refusal STAYS, and names the knob to raise (RFC-0065 D2). Truncation
  is never acceptable: the spin loop inspects only the first N.
- Gates: `check-knob-delivery`, `knob-ends`, `config-knob-census`
  (`KNOB_CLASS`: both new knobs classified `derived`), `executor_backing_claims`
  or whichever test enumerates executor knobs. `just mem-report` before and
  after on the contracted C++ twin phase-462 W1 measured (88 B residue at
  `MAX_MONITORS = 8`); the derived value must not raise the default image.

Gate: a fixture with 14 contracted `min_rate_hz` publishers on one executor
(the island's shape) installs its monitor table on native; a fixture with 15
against an explicit `NROS_EXECUTOR_MAX_MONITORS=14` is refused with the knob
named; `just check fast` green.

Owns: `packages/core/nros-node/{build.rs,src/executor/monitor.rs,src/executor/spin.rs}`,
`packages/api/nros-cpp/src/lib.rs` (the install), the inventory producer,
`scripts/check/config-knob-census.py` (`KNOB_CLASS`), `docs/issues/1471-*`
(status), this document.

Status: claimed 2026-09-25.

## W2 - `NROS_PARAM_SERVICE_INBOX_BYTES` joins the `-1` sentinel

Four sites, all verified 2026-09-24, disagree about what `0` means:

1. `zephyr/Kconfig:1026-1034`: default 0, help "Leave it at 0 to DERIVE it".
2. `zephyr/cmake/nros_cargo_build.cmake:1230-1236` sentinel-guards the two
   sibling inbox knobs (`NROS_SERVICE_INBOX_BYTES`, `NROS_ACTION_INBOX_BYTES`)
   against `-1`; `:1242` forwards THIS one unconditionally with the plain
   resolver. One family, two conventions, eleven lines apart.
3. `packages/core/nros-node/build.rs:565-571` probes with `usize::MAX` for
   "not stated", so a forwarded literal 0 sets `PARAM_SERVICE_INBOX_STATED =
   true` with `PARAM_SERVICE_INBOX_BYTES = 0`; the generated doc on that const
   (`:1010-1013`) says "0 is an absence, not a size". The consumer
   (`parameter_services.rs:1606-1609`) takes the stated branch, and
   `inbox_fits` (`:1663-1670`) cannot catch it because `0 >= 0`.
4. `packages/rmw/zenoh/nros-rmw-zenoh/build.rs:148-151` takes the same 0
   literally as the slot size.

And a fifth, found by the island's memory audit: the generated
`buffer_config.rs` carries `DECLARED_APP_QUERYABLES = usize::MAX` (the
"not stated" sentinel leaking into an emitted constant), which makes
`BUILTIN_INBOX_PER_SESSION` zero, so the parameter family gets NO inbox and
all 26 of the island's queryables fall through `draw_shim_ring` to a
24-byte user-service ring. A `set_parameters` request does not fit that.

What it does: file the issue (next free number; the draft numbered 1471 on
2026-09-24 was never filed and that number is now W1's); give the knob the
`-1` sentinel and the same guard as its siblings; make the two readers agree
that "not stated" is `-1` and never `0`; fix the leak of `usize::MAX` into
`DECLARED_APP_QUERYABLES` so the builtin inbox is sized when the family is
declared; correct the Kconfig help; and prove it on the island's declared
shapes (`5:55`, `4:53`, `12:272:2:37`, `4:57`), whose worst `set_request` is
1015 B, rounded to 1016 for slot alignment. That number must come out of the
derivation, not out of a board file.

Gate: the island's contract configures with NO inbox line in its board file
and the emitted config carries a nonzero builtin inbox of 1016 B; a stated
`CONFIG_NROS_PARAM_SERVICE_INBOX_BYTES=0` is a refusal that says "0 is not a
size; -1 derives"; `check-knob-delivery` names the knob in its pairs;
`just check fast` green.

Owns: `zephyr/Kconfig` (that entry), `zephyr/cmake/nros_cargo_build.cmake`
(that resolver), the two `build.rs` readers, `docs/issues/<n>-*`, this
document.

Status: claimed 2026-09-25.

## What this phase is not

It does not raise any default. A bigger literal moves a constant cost onto
every board image to unblock one (issue 1198's lesson), and this phase exists
because the right value is a function of the contract.
