---
id: 476
title: "`codegen_cyclonedds_descriptors` args-file test flakes with `Text file busy` — a fork/exec race against sibling test threads"
status: open
type: bug
severity: low
area: cli
related: [issue-0471]
---

## Finding

`just ci` failed at `check-cli-tests`, one test out of 509:

```
---- cmd::codegen_cyclonedds_descriptors::tests::nros_codegen_cyclonedds_descriptors_args_file_roundtrip stdout ----
thread '…args_file_roundtrip' panicked at codegen_cyclonedds_descriptors.rs:567:10:
verb runs from args-file: emit cyclonedds descriptors

Caused by:
   0: spawn idlc at /tmp/nros-cli-core-tests-1719023/…/idlc
   1: Text file busy (os error 26)
```

It passes on re-run. The product code is not implicated: the test writes a
stub `idlc` shell script (`write_stub_idlc`, line 410), `chmod 0755`s it, and
the verb then execs it.

## Root cause

`ETXTBSY` on `execve` means some process holds the file open **for writing**.
The test's own handle is not it — `fs::write` closes before returning, and Rust
opens files `O_CLOEXEC`.

The holder is a *sibling test thread's child process*. `cargo test --lib` runs
these tests as threads in ONE process, and several of them spawn subprocesses.
`O_CLOEXEC` closes a descriptor at **exec**, not at **fork**, so between another
thread's `fork` and its child's `exec` the child owns a copy of every open
descriptor in the process — including this test's still-open write handle on
`idlc`. If our `execve` lands inside that window, the kernel sees a writer and
returns `ETXTBSY`.

So the trigger is timing between unrelated tests, which is why it is rare and
why it cleared on re-run.

## Why this is filed rather than fixed

It surfaced while running tier 1 for issue 0471 and is unrelated to it. Fixing
it well means choosing between two options that deserve their own decision:

1. **Test-side:** don't exec the stub directly — invoke it as `bash <stub>`.
   The interpreter opens the script read-only, so `ETXTBSY` cannot arise. Cheap
   and contained, but it stops exercising "the verb spawns a program", which is
   the thing the test is for.
2. **Product-side:** retry the spawn on `ETXTBSY` with a short backoff.
   `ETXTBSY` is genuinely transient, and a real user running `nros` concurrently
   with something that writes the toolchain could hit it — but this is
   production code changed for a test-observed symptom, and the underlying race
   is the harness's, not the product's.

Option 1 is probably right, with a comment naming the race so nobody "fixes" it
back to a direct exec.

## Repro

Rare by nature. `cargo test -p nros-cli-core --lib` in a loop under load; the
window widens with more concurrent spawning tests.
