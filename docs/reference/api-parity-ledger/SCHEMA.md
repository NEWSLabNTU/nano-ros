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

`owed` -- what a `gap` on a name WE DECLARE must carry (issue 1463).

              A `gap` is normally written against a `theirs-only` key, and
              the correlator closes it: when we ship the name the key moves
              and `--check` notices. A `gap` whose subject our side already
              declares (`same`, `systematic`, `arity-only`, `differs`,
              `ours-only`) is a BEHAVIOUR still owed under a name we ship,
              and there the correlator has nothing left to move. Such a row
              MUST carry:

                "owed": {
                  "what":    "the behaviour still owed, one clause",
                  "witness": {"file": "<repo-relative path>",
                              "text": "<literal in that file>"}
                }

              The witness is a literal that exists in our tree BECAUSE the
              behaviour is owed -- the refusal message, the fallback, the
              stub. Doing the work removes it, and `--check` and `--self-test`
              then fail on the row: delete it, or re-point the witness at
              what is still owed. Pick text the fix would have to delete,
              not a line number or a symbol name that survives the fix.

              A DISPOSITION DOES NOT EXEMPT A GAP. The first version of this
              check exempted every dispositioned `gap`, every `gap` carried
              one, and `c:log_severity_t` read LANDED / FIXED in the work
              queue for twelve days. A behaviour bound that is PERMANENT is
              not a `gap` at all -- it is a `divergence` with its constraint.

              `owed` is refused on any other verdict. Nothing reads the date
              a row was last re-measured; the witness is what is checked.

`retired` / `unextracted` -- why a row the extraction does not back is
              kept (issue 1323). `--check` walks the LEDGER as well as the
              differences: every key must name something the extraction
              produced, on either side and in any bucket (a glob: at least
              one key it matches). A row that does not is deleted, unless it
              carries one of these two reasons, each a non-empty string:

                "retired":     "<where the name went, and by which change>"
                "unextracted": "<why the extractor cannot see it>"

              `retired` is for a name that is gone, where the row is the
              record of the rename (a deprecated alias deleted in a batch).
              `unextracted` is for a thing that exists but the extractor
              does not record: a macro, a struct field, an enum variant, a
              derive-provided method, or a row about a concept rather than
              one symbol. Either one on a row whose key the extraction DOES
              produce is red too: the reason it was kept has stopped being
              true, and a stale exemption absorbs the next real defect.

`envelope` -- where an `adopt-bounded` row's envelope is WRITTEN (issue 1637).

              RFC-0089's `adopt-bounded` says "same name and contract, weaker
              inside an envelope the documentation states -- the envelope is
              part of the API". It is the one disposition that asserts
              something OUTSIDE the ledger, so the row names where:

                "envelope": {"file": "<repo-relative path>",
                             "text": "<literal in that file>"}

              The same `{file, text}` shape as `owed.witness`, validated and
              read by the same code. What differs is the direction in time: an
              owed witness exists BECAUSE work is outstanding and the fix
              deletes it; an envelope must outlive every edit. Both are red
              the moment the literal is gone.

              Point at the place a PORTING USER reads -- the doc comment on
              the declaration, the public header, the book page -- and pick
              text that states the bound itself (the limit, the refused
              value, the missing behaviour), not a symbol name that would
              survive the bound being deleted. The ledger's own `why` is not
              an envelope: the user never reads it. `file` must be TRACKED
              (`git ls-files`): a file inside a submodule or an untracked one
              is absent from a fresh clone, and is refused.

              `c:log_severity_t` is why this exists: its row said the envelope
              was stated on `to_facade` and in `log.h`; it was in neither, and
              `to_facade`'s doc said the opposite.

              Required on every `adopt-bounded` row EXCEPT those listed in
              `.config/adopt-bounded-envelope-baseline.txt`, a ratchet that
              may only shrink: a new unwitnessed row is red, and so is a
              baseline line whose row has since gained an envelope, left
              `adopt-bounded`, or gone. If the envelope is documented
              nowhere, the row is wrong -- document the bound or change the
              disposition; do not edit a comment just to hold a string.
              `envelope` is refused on any other disposition. Checked by
              `scripts/api-parity.py --self-test` (fast line), which every
              `--check` runs first.

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

