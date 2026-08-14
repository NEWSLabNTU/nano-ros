---
id: 573
title: "zenohd orphans accumulate for days: two private `RouterHandle` copies
  skip the process-group guard, and a `static OnceLock` never drops them"
status: resolved
type: bug
area: testing
related: [issue-0470, issue-0388]
---

## Symptom

Eleven `zenohd` processes were found alive on the dev host, all reparented to
init, the oldest 3.8 days old:

```
pid=139723  ppid=1 age=329732s  zenohd --listen tcp/127.0.0.1:34909 --no-multicast-scouting
pid=3035967 ppid=1 age=29922s   zenohd --listen tcp/127.0.0.1:40881 --no-multicast-scouting
    (9 more, ages 43211s … 299343s)
```

No test runner was active — no `nextest`, no `cargo`, no `just`. Each held an
ephemeral TCP port for the whole time.

The port-lease directory (`build/nros-tests/port-leases`) was **empty** while all
eleven were running. That is the tell: these routers never took a lease, so they
were not started by the `ZenohRouter` fixture at all.

## Root cause

`packages/rmw/zenoh/nros-rmw-zenoh/tests/cffi_smoke.rs` and
`.../tests/status_events_matrix.rs` each carry a private `RouterHandle` that
spawns zenohd itself instead of using `nros_tests::fixtures::ZenohRouter`. Both
copies are byte-for-byte the same shape, and both are wrong in the same two
independent ways.

### 1. No process-group guard, so nothing kills the orphan

The shared fixture arms every child through
`nros_tests::process::set_new_process_group`, which does:

```rust
libc::setpgid(0, 0);
libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
```

`PR_SET_PDEATHSIG` is the entire orphan defence: when nextest SIGKILLs a test
binary, `Drop` cannot run, and the kernel is the only thing left that can reap
the child. The private copies call plain `Command::spawn()` with no `pre_exec`,
so their zenohd has no death signal armed and simply keeps running.

The mechanism itself is sound — verified directly, replicating the `pre_exec`
body (fork → `setpgid(0,0)` → `prctl(PDEATHSIG, SIGKILL)` → `execv`) and then
SIGKILLing the parent: the child dies, in that arm order and in the reverse one.
So this is not a broken guard, it is an *unguarded spawn path*.

### 2. The `impl Drop` is dead code

Both handles are stored in a process-lifetime static:

```rust
static ROUTER: OnceLock<Mutex<Option<RouterHandle>>> = OnceLock::new();
```

Rust does not run destructors for `static`s at process exit. So
`impl Drop for RouterHandle` never executes — not on panic, not on SIGKILL, and
not on a **clean** exit either. Every normal green run of these two test
binaries leaks a router.

That is why the ages are spread across days rather than clustered: this leaks
once per run of either binary, unconditionally, and nothing ever collects them.

### 3. It re-introduces the issue-0470 port race

Both copies bind port 0, read the number, and close the socket before spawning:

```rust
let listener = TcpListener::bind("127.0.0.1:0").ok()?;
let port = listener.local_addr().ok()?.port();
drop(listener);
```

`status_events_matrix.rs` even documents it as "Race-y but good enough for a
single-shot test fixture". This is exactly the allocator issue 0470 removed from
the shared fixture, for exactly the reason recorded there: between the `drop` and
zenohd's own bind, the kernel will hand the same port to a concurrent caller.
Issue 0470 measured 87 collisions in 2400 allocations across 12 processes.

## Why it survived so long

The leak is invisible from inside a test run. A leaked router does not fail the
test that leaked it, and `ZenohRouter::kill_listeners_on_port` — the one
collector that exists — only fires when a *later* fixture is handed that same
port. A router sitting on a port nobody asks for again is never looked at by
anything. Nothing in CI counts live zenohd processes, so a green sweep and a
sweep that leaked four routers are indistinguishable.

This is the shape CLAUDE.md warns about under "fix the CLASS": the fixture was
hardened by issue 0470 and issue 0388, and both hardenings applied to the one
call site everyone knew about while two private copies kept the original
defects.

## Resolution

Delete both private `RouterHandle` copies and route these tests through
`nros_tests::fixtures::ZenohRouter::start_unique()`, which already carries the
process-group guard, the port lease and the graceful SIGTERM→SIGKILL teardown.
One spelling, not three.

Keeping the router in a `static OnceLock` stays acceptable *after* the switch,
because `PR_SET_PDEATHSIG` — not `Drop` — is what actually bounds the child's
lifetime when the parent is killed or when a static is never dropped.

Add a gate so a fourth copy cannot appear: any `Command` spawning
`zenohd_binary_path()` outside `fixtures/zenohd_router.rs` is a failure.

## Not fixed here

`set_new_process_group` arms `PR_SET_PDEATHSIG` inside `pre_exec`, i.e. after
`fork()`. If the parent dies in that window the signal is never armed and the
child leaks anyway. The window is microseconds and cannot explain eleven
orphans, so it is noted rather than addressed; the robust form re-checks
`getppid()` after arming and exits if it already changed.

## Verified

Both suites green through the shared fixture, and no router survives them:

```
zenohd before: 0
test result: ok. 2 passed; 0 failed; 1 ignored   (cffi_smoke)
test result: ok. 1 passed; 0 failed; 0 ignored   (status_events_matrix)
zenohd after:  0
```

The static is still never dropped — the router dies because the test binary
exits and `PR_SET_PDEATHSIG` fires, which is the mechanism this issue is about.

Gate `check-zenohd-spawn-sites` (wired into `check-fast`) was checked against
the pre-fix tree and fails on both removed sites:

```
ERROR: .../cffi_smoke.rs spawns zenohd directly — use nros_tests::fixtures::ZenohRouter
ERROR: .../status_events_matrix.rs spawns zenohd directly — use ...
```

The eleven orphans found on the host were killed (`ppid=1`, no runner active).
