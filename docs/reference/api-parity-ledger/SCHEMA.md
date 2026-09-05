# api-parity ledger — schema

The ONE home for what a ledger row means: verdicts, key spelling, required
fields. It used to be copied into the `_doc` array of all 17 shards, so a
schema change touched 17 files and two concurrent PRs conflicted in up to 17
paths without disagreeing about anything (issue 1095).

Each shard's `_doc` points here and carries only notes specific to that shard.

Phase 379. One row per item where the nano-ros user API does NOT correspond
to the ROS 2 client library it mirrors. Written by hand; read by
`scripts/api-parity.py --check`, which fails on any non-matching row that has
no entry here.

Key is '<lang>:<normalized item>' exactly as the report prints it, where lang
is one of c / cpp / rust. Run the report to get the spelling; do not guess it.

verdict is one of:
  divergence  we changed it and a PLATFORM CONSTRAINT is why. `why` must name
              the constraint (no_std, no exceptions, no allocator, no runtime
              env, single-threaded transport) -- not a preference. This is the
              only sanctioned reason to differ (RFC-0036).
  extension   we add it because an RTOS scenario needs it; ROS 2 has none.
  declined    ROS 2 has it, we deliberately do not, with the reason.
  gap         ROS 2 has it, we should too, nobody has done it. A gap is a
              legitimate entry -- the point is that it is written down.
  rename      the names differ and OURS is the one that should change. A
              rename with no platform reason costs the drop-in claim for
              nothing, so these are the campaign's work list.
  their-rename
              the names differ, the CAPABILITY matches, and THEIRS is the
              one that should change: ours is the spelling the broader ROS 2
              ecosystem already uses and the library this lane compares
              against is the outlier. The mirror of `rename`, and the only
              verdict that says a difference is UPSTREAM's to close.

              A claim about NAMES ONLY. If the shapes also differ for a
              platform reason the row is a `divergence`; if we do not ship
              the capability at all it is `declined` or `gap`; if we ship it
              and ROS 2 does not it is an `extension`. Both halves of a pair
              carry it -- the theirs-only row and the ours-only row are one
              statement seen from two sides.

              A row MUST carry a `their_rename` object, and `--check`
              refuses the verdict without it:
                ours      our spelling.
                majority  a list, each entry naming a ROS 2 spelling ours
                          agrees with and where it is recorded. At least one
                          entry must cite an upstream (rcl / rclcpp / rclrs /
                          rclc / an interface package) -- our own three
                          languages agreeing is internal consistency, which
                          is a PREFERENCE, and preferences are not this
                          verdict. `c:trigger_guard_condition` stays
                          `declined` for exactly that reason: `nros_<entity>_
                          <verb>` is our convention, not ROS 2's majority.
                outlier   the one upstream spelling that differs, with the
                          evidence it is the minority -- countable in
                          docs/reference/api-surface/ or in an interface
                          package (rcl has 14 `rcl_*_is_valid` and exactly
                          one `rcl_clock_valid`).
                pair      the ledger key of the other half, when there is one.

              It does NOT cancel the drop-in cost -- a ported node still has
              to be edited, and `why` should say what the reader loses. What
              it buys is that `rename` stays a list of OUR defects, and that
              a row cannot say "we have it" (`declined`) or "ROS 2 has none"
              (`extension`) when neither is true.

              `scripts/check-prelude-tiers.py` treats it as HAVING an
              upstream correspondent, so a `their-rename` name is prelude-
              eligible; only `extension` is excluded there.

This file is SEEDED, not complete: W1 shipped the correlator, W2 classifies
the rest. `--check` is deliberately not wired into `just check` until then --
a gate that fails on ~2000 rows from the day it lands is one somebody
switches off. Keys beginning with '_' are documentation and are skipped.

the `<lang>:` rows for its own lane, and `scripts/api-parity.py` merges every
shard in this directory. One agent per lane can write without a rebase
conflict against the others.

SHARDED BY TOPIC: this file holds one stage's rows in ALL THREE
languages, because the campaign closes a feature at a time across every
language. `scripts/api-parity.py` merges every shard here, and
`--self-test` rejects a row whose item belongs to a different stage.

