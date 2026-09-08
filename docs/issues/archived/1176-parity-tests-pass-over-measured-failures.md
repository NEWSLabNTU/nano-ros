---
id: 1176
title: "three `parity_test.rs` tests MEASURE parse failures and then pass — and the report they print is captured by libtest, so nobody sees it"
status: resolved
type: bug
area: testing, codegen
severity: high
found: 2026-09-06
resolved: 2026-09-08
related: [0693, 1135, 1160, 1230]
---

# Not an unmet precondition — a measured failure, reported as success

`packages/cli/rosidl-codegen/tests/parity_test.rs`, three tests
(`test_parse_all_*`, at lines 254, 311 and 368). Each walks a ROS share root,
parses every `.msg`/`.srv`/`.action`, collects what failed — and then:

```rust
if !failures.is_empty() {
    eprintln!(
        "Failed to process {} out of {} std_msgs ({}% success rate):",
        failures.len(), count, (count - failures.len()) * 100 / count
    );
    for failure in &failures {
        eprintln!("  {}", failure);
    }
    // Don't panic - just report the failures
    eprintln!("Note: Some failures expected due to parser limitations (default values, etc.)");
}
```

This is a different and worse class than issues 1135 and 1160, which were both
*unmet preconditions* reported as PASS. Here the precondition was MET, the work
ran, the failures are real and counted, and the verdict is still green.

## And the report is invisible on the lane that runs it

libtest captures the output of a **passing** test, and `check-cli-tests` runs
`cargo test --manifest-path packages/cli/Cargo.toml --workspace --quiet` — no
`--nocapture`. So the `eprintln!` block reaches nobody. Measured on a ROS-less
host during issue 1160's sweep: `parity_test` reported `16 passed` in 0.00 s
with no `[NO-ROS]` or failure line anywhere in the output.

Same root shape as the mitigation issue 0693 recorded for these suites — an
`eprintln!` standing in for a skip — and the same reason it does not work.

## Why the comment is not a defence

"Some failures expected due to parser limitations (default values, etc.)" may
well be true. But an expected-failure set that is neither enumerated nor bounded
is indistinguishable from a regression:

* a parser change that breaks 40 more messages passes identically;
* a parser change that FIXES all of them also passes identically, so nobody
  learns the limitation is gone;
* the percentage in the message is computed and then discarded.

## Fix — the shape, not the threshold

Make the tolerated set explicit, so a change to it is a diff someone reviews:

1. **An enumerated expected-failure list** (per message path, with the reason —
   "default values unsupported"), asserted exactly: an unexpected failure fails
   the test, and an entry that now PARSES also fails, so the list cannot rot.
   That is the ratchet idiom this repo already uses elsewhere.
2. Failing that, a **bounded** assertion — `failures.len() <= N` with `N`
   committed and a comment naming what is in it. Weaker, because it cannot tell
   you the set changed identity at constant size, but still a verdict.

Prefer (1). Either way the numbers belong in an `assert!` message, where a
failing test prints them, rather than in an `eprintln!` on the passing path.

## The adjacent item, recorded separately

The same files (`compilation_test.rs`, and `bare_metal_link.rs` in
`nros-rmw-cyclonedds`) **compile at test runtime**, against CLAUDE.md's "No
compilation inside tests — compile in the build stage, the test consumes the
prebuilt fixture". That is its own class and its own decision; noted here only
so the next reader of these files is not surprised by it.

It is **issue 1230** now, so the record survives this file's archival. One
correction went in with it, from reading the code rather than the note: the two
`bare_metal_link.rs` tests both carry `#[ignore]` with an explicit "heavy:
invokes cargo build" reason, so their compilation is opt-in and labelled, unlike
the four in `compilation_test.rs` that run on every `cargo test`.

## RESOLVED — 2026-09-08

### All three claims held, and one number moved

* The three sites are real: `parity_test.rs` lines 203, 254 and 305 at the time
  of the fix (the issue's 254/311/368 predate 1160 shrinking the file), three
  literal copies of the same block.
* The report really is invisible. `just check cli-tests` is
  `cargo test --manifest-path packages/cli/Cargo.toml --workspace --quiet`, no
  `--nocapture`; measured here before the fix,
  `cargo test -p rosidl-codegen --test parity_test --quiet` printed
  `17 passed … finished in 0.00s` and nothing else. (17, not the issue's 16 —
  1160 added `guard_decides_by_state` in between.)

### The excuse was measured, and it was false on both counts

"Some failures expected due to parser limitations (default values, etc.)"
described nothing that exists.

1. **The corpus.** `packages/cli/interfaces/` is a vendored copy of the ROS 2
   Humble sources for exactly these packages — it is there so codegen works on a
   ROS-less host. Walked with the same parse-and-generate the tests do: **133
   `.msg` files across 10 packages, zero parse failures, zero generate
   failures** (std_msgs 30, geometry_msgs 33, sensor_msgs 27, example_interfaces
   29, lifecycle_msgs 4, action_msgs 3, diagnostic_msgs 3, builtin_interfaces 2,
   rosgraph_msgs 1, unique_identifier_msgs 1).
2. **The named limitation.** `parse_message` accepts a scalar default
   (`int32 x 42`), a string default (`string name "hi"`), an array default, a
   bounded string (`string<=10 s`), a bounded array (`int32[<=5] a`) and a
   constant. All six measured directly.

So the honest tolerated set is empty, and it is empty as a MEASUREMENT rather
than as an assumption — which is the difference between this and shipping a
`failures.len() <= N` whose `N` nobody could verify.

### The fix

Approach (1) from the Fix section: an enumerated ledger, asserted exactly.

* `packages/cli/rosidl-codegen/tests/parity-expected-failures.txt` — committed,
  currently zero entries, with the measurement above in its header. Format
  `<package>/<kind>/<file>  # <reason>`; a key is relative to a share root, so it
  is the same string on every host. An entry with no reason is rejected.
* `tests/parity_ledger.rs` — ONE walk (it was three, copied per package, which
  is how one defect came to exist three times) plus `parity_verdict`, which
  compares a walk to the ledger in BOTH directions: an unlisted failure is red,
  and a listed entry whose file was visited and SUCCEEDED is red too, so the set
  cannot rot. The failing assertion prints the lines to add or delete in the
  ledger's own format, verbatim-pasteable.
* The three `test_parse_all_*` tests are now three lines each.
* A walk that finds no `.msg` in a directory that RESOLVED is a hard failure —
  the old code divided by that zero.

### The ratchet needed a lane, or it would have been most of the way back here

The three ROS-host tests can only answer on a host with ROS, and
`check-cli-tests` deliberately has none ("It needs no ROS either, and that is a
property of the suite rather than an assumption" — `gate.yml`). A ratchet no
lane fires is not much better than a swallowed `eprintln!`. So
`bundled_interfaces_have_no_parity_failures` runs the same walk over the
vendored corpus, on every host including CI: 133 definitions, 0.8 s, a real
verdict where there was none.

### Demonstrated both ways, on a ROS-less host

1. clean → green (267 tests, all pass).
2. a malformed `.msg` dropped into the vendored corpus → RED, printing
   `std_msgs/msg/ZzDemoBroken.msg  # parse failed: UnexpectedToken { expected: "array size or ]", got: "[" }`.
3. that exact line pasted into the ledger → green, one tolerated.
4. a ledger entry for a file that succeeds (`std_msgs/msg/Bool.msg`) → RED,
   `1 LEDGER ENTRY/ENTRIES THAT NOW SUCCEED … delete these lines`.

`parity_ledger`'s own unit tests exercise all four states plus the
"listed but not carried by this host" case, on any machine, with no ROS —
the `guard_decides_by_state` idiom 1160 established for the same reason.

### The sibling that swallowed its own verdict

`packages/cli/rosidl-codegen/scripts/check_parser_failures.sh` scraped the
`eprintln!` this issue deletes, so it had to move. Reading it turned up a second
instance of the same class, unnoticed since 0693 fixed the first: it gated each
package on `grep -q "0 filtered out"`, but `cargo test --test parity_test
<one-name>` filters the OTHER tests out, so the output reads `21 filtered out`,
every package took the `⊘ No test for this package` arm, and the script divided
a failure count of 0 by a message count it had gathered from the filesystem and
announced **"Success rate: 100%"** over ten packages it had not checked. It now
detects whether the test RAN (libtest's own `test <name> ... <verdict>` line),
surfaces the ledger report, drives the bundled walk so it does real work on a
ROS-less host, names unchecked packages instead of folding them into a
percentage, and exits non-zero.

### What could NOT be measured

There is no ROS on the host this was fixed on, so the three `/opt/ros` walks
were never executed against a real distro — only against the vendored copy of
the same Humble sources. Two consequences, stated rather than papered over:

* if an installed distro carries a `.msg` the vendored corpus does not, its
  first run will be RED with a pasteable line, which is the intended behaviour
  and not a regression;
* a ledger entry naming a file the host does not carry is tolerated in silence.
  That is deliberate — the repo does not pin which distro is installed, so an
  entry true on jazzy is not wrong on humble — and it hides nothing, because a
  regression is always an UNLISTED failure.

## Provenance

Found by issue 1160's sweep of precondition guards in `packages/*/tests/`,
which fixed four real PASS-on-failure defects and deliberately left this one as
a different class.
