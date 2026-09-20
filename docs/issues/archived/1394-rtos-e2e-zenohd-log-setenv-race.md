---
id: 1394
title: "`enable_router_session_log` calls `std::env::set_var` inside a
  multi-threaded test binary, a `setenv`-during-`getenv` hazard"
status: resolved
type: bug
area: testing
severity: low
found: 2026-09-20
related: [issue-1313, issue-1056, issue-0906]
resolved_in: "branch fix/1394-rtos-e2e-setenv-race"
---

## What is true

`packages/testing/nros-tests/tests/rtos_e2e.rs:875`:

```rust
fn enable_router_session_log() {
    if std::env::var_os("ZENOHD_LOG").is_none() {
        // SAFETY: single-threaded, once, at the top of the test.
        unsafe { std::env::set_var("ZENOHD_LOG", ROUTER_SESSION_LOG_FILTER) };
    }
}
```

`ZenohRouter::start_on` reads `ZENOHD_LOG` when it SPAWNS the router
(`src/fixtures/zenohd_router.rs:285`) and there is no per-call switch, so the
test steers the router by writing the process environment.

The safety comment's premise is a property of the RUNNER, not of the code. It
holds under nextest, which gives each case its own process. It does not hold
under plain `cargo test --test rtos_e2e`, where libtest runs the generated
cases on threads of one process. `set_var`/`remove_var` are `unsafe` in edition
2024 precisely because glibc's `setenv` may reallocate `environ` while another
thread is inside `getenv`; every sibling case in this binary spawns QEMU guests
and host processes, and process spawn itself reads the environment.

## What was measured, and what was not

This was surveyed during issue 1313 and deliberately left out of that fix. The
survey made four claims; all four were re-checked on `origin/main`
(`811f081a3`) before this issue was filed.

| claim | verdict |
| --- | --- |
| no sibling writes or clears `ZENOHD_LOG` | **HELD.** `rg -n ZENOHD_LOG` over the repo matches two READS (`zenohd_router.rs:285`, `:331`), this one WRITE, one assertion message, and prose in docs. Nothing else writes it |
| the write is guarded on "still unset" | **HELD.** `var_os(..).is_none()` |
| its one `#[rstest]` caller's cases all write the same constant | **HELD.** `enable_router_session_log` has exactly one call site, `test_rtos_pubsub_e2e` (line 1074); its 4 platforms x 3 languages = 12 generated cases all write `ROUTER_SESSION_LOG_FILTER`. Writers cannot disagree |
| the value selects only whether the router keeps a log, never a verdict | **HELD for the hazard, with one caveat worth recording.** No value this code writes can change a verdict. But an OPERATOR value is deferred to, and a value that is not a superset of `zenoh_transport=debug` — `ZENOHD_LOG=info`, say — makes `assert_no_session_churn` fail its `sessions >= 2` arm, and the failure text blames the filter constant or a zenoh rename rather than the operator's variable. That is a diagnosis quality defect, not the race |

**No wrong verdict has been observed from this call.** There is no flake to
point at, no failing run, and no reproduction — unlike issue 1313, which had a
measured 49/60 failure rate once its filter was narrowed. The two arms of 1313
had to write what the other had to clear; this one has a single writer, so the
observable symptom it could produce is not a wrong answer but a process-level
memory fault, which would present as a crash or corruption in an unrelated
frame and would never be attributed here.

So this is filed on the FORM of the call, not on damage. Severity `low`: a test
target, no observed failure, and the runner the lane actually uses (nextest)
makes the premise true.

## Why it was not folded into 1313

1313's own preferred fix — inject the value instead of reading process env —
was judged too expensive: "injecting it is a change across ~64 spawn sites in a
fixture-gated target."

The count is real (74 `ZenohRouter::start*` call sites under `packages/`,
measured) but the inference does not follow. The filter only has to reach
`start_on`, and this test reaches `start_on` through exactly ONE helper,
`Platform::zenoh_router_start`, which has **3** call sites in `rtos_e2e.rs`. A
defaulting wrapper leaves every one of the other 73 signatures untouched.

## Fix

Injection, as 1313 preferred, sized honestly:

* `ZenohRouter::start_on_with_log_filter(bind_addr, port, Option<&str>)` and a
  `start_slirp` twin; the existing `start_on` / `start_slirp` become
  `..._with_log_filter(.., None)`, so no existing caller changes.
* One resolver for the decision the env read used to make: operator
  `ZENOHD_LOG` first, else the caller's injected filter, else nothing. That is
  the existing "an operator value is left alone" semantics written as a
  fallback rather than as a guarded mutation.
* `Platform::zenoh_router_start` takes the filter and passes it down; the
  pubsub case supplies `Some(ROUTER_SESSION_LOG_FILTER)`, the service and
  action cases supply `None` — which is what they get today.

`enable_router_session_log` then has nothing to do and is deleted.

The alternatives were weighed and rejected:

* **A `OnceLock`/`Once` that sets it exactly once.** Makes the write happen
  once per process, which it already does (the caller is one function with one
  guarded write). It does not address the hazard at all, which is a write
  concurrent with another THREAD's read, not a repeated write.
* **`EnvGuard` + a static lock, as `tests/init_api.rs` uses.** The right shape
  THERE, because the code under test reads `ROS_DOMAIN_ID` &c. from process env
  by design and there is nothing to inject. Here there is something to inject.
  And the lock would be weaker than it looks: it serialises writers against
  each other, while the readers that matter are libc and `Command::spawn` in
  unrelated sibling cases, which take no such lock.

## Sweep

`rg -n 'set_var|remove_var' packages/testing/nros-tests`

| site | verdict |
| --- | --- |
| `tests/rtos_e2e.rs:878` | **this issue** |
| `tests/init_api.rs:42,49,59,60` | **SAFE, unchanged.** `EnvGuard` + `env_lock()`, taken by every live `#[test]` in the target; the one case that does not lock is `#[ignore]`d with an empty body. Re-verified here: 4 `#[test]`s, 3 live and all 3 lock. 1313's option 2 done properly, and the correct shape there |
| `src/fixtures/lane.rs:538`, `src/fixtures/binaries/mod.rs:616,625,6106,6975` | **PROSE.** Doc comments and one code comment, three of them describing 1313 itself. No calls |

No CALL of `set_var`/`remove_var` exists anywhere in
`packages/testing/nros-tests/src`; 1313 left that clean.

## Acceptance

* `packages/testing/nros-tests/tests/rtos_e2e.rs` contains no `set_var`.
* `cargo test -p nros-tests --test rtos_e2e --no-run` builds.
* The router's log behaviour is unchanged for every caller that does not ask
  for a filter, and an operator `ZENOHD_LOG` still wins.

---

## Resolution (2026-09-20)

Fixed by **injection**, the option 1313 preferred and judged unaffordable here.
No mutex, no `Once`, and no `set_var` left in the target.

### What changed

`packages/testing/nros-tests/src/fixtures/zenohd_router.rs`

* `ZenohRouter::start_on_with_log_filter(bind_addr, port, Option<&str>)` and
  `start_slirp_with_log_filter(port, Option<&str>)` are new. `start_on` and
  `start_slirp` call them with `None`, so **all 73 other
  `ZenohRouter::start*` call sites compile unchanged**.
* `resolve_router_log_filter(caller)` is the one place the precedence lives,
  and both log-keeping constructors (`start_on`, `start_serial`) go through
  it. It delegates to a pure `router_log_filter(operator, caller)` so the rule
  can be asserted by a unit test that touches no environment — a test that
  wrote env to test an env-write fix would be the bug again.

`packages/testing/nros-tests/tests/rtos_e2e.rs`

* `enable_router_session_log` is deleted.
* `Platform::zenoh_router_start` grew a `log_filter` parameter. Three call
  sites: pubsub passes `Some(ROUTER_SESSION_LOG_FILTER)`, service and action
  pass `None` — which is what they effectively had.
* Two messages in `assert_no_session_churn` name the new seam, and the "fewer
  than two session lines" failure now names the third cause it could not see
  before: an operator `ZENOHD_LOG` is deferred to by design, and
  `ZENOHD_LOG=info` selects none of those lines.

### Behaviour delta

Under nextest — the runner these lanes use — **none**: each case is its own
process, so the deferred-to operator value and the cell's own filter resolve
exactly as before.

Under plain `cargo test --test rtos_e2e` there is one change, and it is a
removal: the pubsub cases no longer leak `ZENOHD_LOG` into the process, so the
service and action cells in the same binary stop inheriting a log they never
asked for. Nothing relied on that; it was a side effect of the mechanism.

### The cost claim, measured

| claim | measured |
| --- | --- |
| "~64 spawn sites" (issue 1313's note) | 74 `ZenohRouter::start*` call sites under `packages/` — the count is real |
| sites that actually had to change | **5**: two constructors gained a defaulting wrapper, and `zenoh_router_start` plus its 3 call sites took a parameter |

The inference, not the arithmetic, was the error: the filter only has to reach
`start_on`, and the test reaches `start_on` through one helper.

### Alternatives, and why not

* **`OnceLock`/`Once`.** Would make the write happen once per process — which
  it already did. The hazard is a write concurrent with another thread's read,
  not a repeated write, so this addresses nothing.
* **`EnvGuard` + static lock (`tests/init_api.rs`'s shape).** Right there,
  wrong here. It is 1313's "option 2 done properly" for code that reads process
  env BY DESIGN and has nothing to inject; this code had something to inject.
  It would also be weaker than it looks: it serialises writers, while the
  readers that matter are libc and `Command::spawn` in sibling cases, which
  take no lock.

### Sweep

`rg -n 'set_var|remove_var' packages/testing/nros-tests`

| site | verdict |
| --- | --- |
| `tests/rtos_e2e.rs:878` | **GONE** — the function it lived in is deleted |
| `tests/init_api.rs:42,49,59,60` | **SAFE, unchanged.** `EnvGuard` + `env_lock()`; re-verified, 4 `#[test]`s, 3 live, all 3 take the lock, the 4th `#[ignore]`d with an empty body |
| `src/fixtures/lane.rs`, `src/fixtures/binaries/mod.rs` (5 hits) | **PROSE.** Doc and code comments; the `binaries/mod.rs` one that described this site as "surveyed, not fixed" now records the fix and the corrected cost |

No `set_var`/`remove_var` CALL remains anywhere in
`packages/testing/nros-tests` outside `init_api.rs`'s documented discipline.

### Verified

| | result |
| --- | --- |
| `cargo test -p nros-tests --test rtos_e2e --no-run` | builds |
| `cargo test -p nros-tests --lib` | **216 passed, 0 failed** |
| the new `an_operator_log_filter_outranks_the_one_a_cell_asks_for` | passes; all three arms |
| `cargo clippy -p nros-tests --tests -- -D warnings` | clean |
| `just check fast` | **324 of 327 green**; the 3 reds are this worktree's provisioning — `capability-conditionals` and `xrce-vendored-versions` name uninitialised submodules in their own output, and `codegen-version-refusal` check E emits 3 against a tree at 6 because the in-worktree `nros` CLI was never built. None reads `packages/testing/` |

**What was NOT verified, and cannot be here:** no cell of `rtos_e2e` ran. All
12 pubsub cases fail in `build_pair` with `FixtureNotBuilt` before a router is
ever started, on an unprovisioned host — so the changed line has **zero runtime
coverage** from this work, and the evidence above is compile-time plus the unit
test of the precedence rule. Running it needs `just build-test-fixtures` and a
QEMU/RTOS toolchain.
