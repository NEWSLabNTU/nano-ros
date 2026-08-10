---
id: 471
title: "`wait_for_output_pattern` returns Ok on timeout whenever the process printed anything, so ~233 call sites assert nothing"
status: resolved
resolved_in: f46c92840
type: bug
severity: high
area: testing
related: [issue-0469, issue-0465, issue-0445, issue-0196]
---

## Finding

`ManagedProcess::wait_for_output_pattern(pattern, timeout)` reads as an
assertion — "wait until this marker appears" — and every caller uses it that
way. It is not one. On timeout:

```rust
// packages/testing/nros-tests/src/process.rs
if start.elapsed() >= timeout {
    self.handle.stdout = stdout;
    self.handle.stderr = stderr;
    if output.is_empty() {
        return Err(TestError::Timeout);
    }
    return Ok(output);          // <- pattern never matched
}
```

The pattern is consulted only for the EARLY-EXIT path. If the timeout expires
and the process printed **anything at all** — including the error explaining why
it failed — the call returns `Ok`.

So `wait_for_output_pattern(MARKER, …)?` means "the process was not completely
silent", not "the marker appeared".

## How it was found

Writing `port_templates_e2e` (issue 0469) for a bug whose signature is known
(0465: the node dies at startup printing `Transport(InvalidConfig)`). The test
was checked against the broken fixture before being trusted — and **passed**.
The failing node's error line is non-empty output, so `Ok`.

Had that check been skipped, the repo would have gained a test that looks like
it guards phase 209's acceptance and cannot fail — the same class as the gate
whose coverage is narrower than its rule (0196), and the verdict that absorbs
the result behind it (0445).

## Blast radius

```console
$ git grep -c wait_for_output_pattern -- packages/testing/nros-tests/tests | awk -F: '{s+=$2} END {print s}'
283
$ # sites that ignore the returned string and check only the Result:
233
```

**233 of 283 call sites never look at the returned text.** Not all are wrong:
many are genuine readiness waits ("wait for `Spinning`, then do the real
assertion"), where a lenient wait is harmless and the test still fails later.
But the ones where the marker IS the assertion — e.g.
`action_multigoal.rs:75` waiting 60 s for `MULTIGOAL_SUMMARY_PREFIX`,
`bridge_*` waiting for a forwarded-count line — are vacuous today, and no
inspection distinguishes the two categories without reading each test's intent.

## Fix direction

The honest contract is that a function named `wait_for_output_pattern` returns
`Err(Timeout)` when the pattern did not appear; the `output.is_empty()` special
case is the defect. That is a one-line change, and deliberately NOT made here:
flipping it converts every vacuous assertion into a failure at once, and those
failures are a mix of

* stale markers (the phase-277 class — the fixture prints something else now),
* genuine product regressions that have been hiding, and
* readiness waits that were always meant to be lenient.

Separating those is the work, and it wants its own pass with the tier green
before and after — not a flag day inside another fix. Sequence:

1. land the contract change behind a temporary strict variant,
2. migrate the readiness waits to the lenient one explicitly,
3. flip the default and triage what turns red,
4. delete the temporary variant.

Step 3 is where the value is: every red is either a test that never tested
anything or a bug that has been passing CI.

## Resolution (2026-08-07) — the flip, and what it caught

All four steps in the sequence above are done, in one pass, because step 3
turned out to be small and entirely one class.

### The contract

`wait_for_output_pattern` is now STRICT: `Ok` means the pattern appeared,
`Err` means it did not, and the error quotes the output so the failure explains
itself. `collect_until(pattern, timeout) -> String` is the LENIENT counterpart
under an honest name — it is exactly the old behavior, returns the output
whether or not the pattern showed up, and is the right call for a readiness
wait or a test that asserts on the content itself.

Both are one engine, `wait_until_pattern -> (String, bool)`. That shape is the
actual fix: the defect existed because a single `Result` conflated "what was
printed" with "did it match", so the only path that carried the output was also
the path that claimed success.

The temporary `expect_output_pattern` variant is deleted — it never gained a
caller, and with the real function strict it was a synonym.

### Two corrections to the finding above

* **The blast radius was overstated.** "233 call sites assert nothing" counts
  sites that ignore the returned string, but ignoring the `String` is not the
  same as asserting nothing. `action_multigoal.rs:75` is cited above as
  vacuous; it is not — it binds the output, finds the summary line, and panics
  if absent. Static breakdown of the 285 sites: **124 bound and inspected**
  (safe under either contract), 14 bound but unread, 147 discarded. What
  actually mattered was the ~92 sites that `.expect()` the result, since those
  are the ones a strict contract can newly fail.
* **There were TWO lenient returns, not one.** The finding names the timeout
  path. The process-EXIT path (`break` out of the loop, then `Ok(output)`) was
  equally lenient: a process that died without ever printing the marker also
  reported success. On an emulator target that is the common path.

And the same defect, in the same two shapes, was in **`QemuProcess`**
(`qemu.rs`) — found by looking for siblings rather than fixing the reported
site. `wait_for_output_count`, by contrast, was already strict on both paths
with a good error; it is the one that got it right, which is why the fix copies
its shape rather than inventing one.

### What the flip caught

Tier 1, strict, before any migration: **25 raw failures, of which 7 are
`skip!` panics** (bare nextest counts those as failures) → **18 real**, against
a baseline of 3 known reds. So 15-16 new — and **every one of them was the same
bug**:

> Tests waited for the literal `"Waiting for"`, a banner
> `examples/native/rust/listener` **does not print**. Its readiness line is
> `Subscriber created for topic: /chatter` — as that example's own source
> comments say. phase-277 slimmed the banner; the tests were never updated,
> and nothing noticed for months because the wait returned `Ok` on timeout.

Affected: `multi_node` (7), `qos` (5), `interop_e2e` case 2, `bridge_mixed_rmw`,
`safety_e2e`, `cpp_multi_node_entry`. All fixed by using constants —
`output::LISTENER_READY_MARKER`, `output::SAFETY_LISTENER_READY_MARKER` (the
safety listener spells it `Safety subscriber`, so the plain marker is NOT a
substring of it), and `INT32_TALKER_LOG_PREFIX` for the C++ entry, which logs
`Published: N` and never had a `"Waiting for messages"` banner at all.

This is precisely the rule CLAUDE.md already states — *test greps use
`nros_tests::output::*` constants, never literal strings* (phase-277) — and the
lenient wait is why breaking it was free.

**Not a blanket rename.** Most binaries still DO print `"Waiting for"`: every
C and C++ listener, the service servers, the baremetal listeners. Only the Rust
chatter listener dropped it. A global replace would have broken the majority to
fix the minority, so each site was changed on the evidence of its own failure,
and `error_handling.rs` was fixed alongside as the same binary and same class.

### Side effect: the suites got faster

A readiness wait for a marker that never arrives burns its whole timeout. `qos`
and `multi_node` each paid 5 s per listener on every run. Post-fix those tests
complete in 1-3 s.

### Residuals

* `nano2nano::test_peer_mode_communication` now reaches its own
  `skip!("peer mode may not be supported — listener exited early")`, which the
  lenient wait had made unreachable — the guard is `is_err() && !is_running()`
  and `is_err()` was previously never true. The skip is honest, but it means
  the test no longer asserts anything in this environment. Whether the
  peer-mode listener SHOULD exit is a separate question; standalone it stays
  up.
* `entry_e2e::entry_matrix` is issue 0460, pre-existing and unrelated.
* Remaining literal `"Waiting for"` call sites (xrce, zephyr, esp32,
  zero_copy, native_api, …) target binaries that genuinely still print it. They
  are not stale today, but they are literals, and the strict contract is now
  what would catch it if their banner is ever slimmed.

### Regression tests

`process.rs` unit tests cover both lenient paths and the lenient variant:
non-matching output on timeout must `Err` and must quote the output; exit
without the pattern must `Err`; a real match must `Ok`; `collect_until` must
return the output regardless. Verified they FAIL against the old contract
(simulated by restoring the `!out.is_empty() => Ok` arm) and pass against the
new — the check that was skipped when the original defect shipped.
