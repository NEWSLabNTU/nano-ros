---
id: 476
title: "Writing an executable stub and exec'ing it races against sibling test threads (`Text file busy`) — unique paths do not fix it"
status: resolved
resolved_in: d88aea3d8
type: bug
severity: low
area: cli
related: [issue-0455, issue-0471]
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

It passes on re-run. The product code is not implicated: the test writes a stub
`idlc` shell script, `chmod 0755`s it, and the verb then execs it.

## Root cause — confirmed by experiment

`ETXTBSY` on `execve` means some process holds the file open **for writing**.
The test's own handle is not it: `fs::write` closes before returning.

The holder is a **sibling test thread's child process**. `cargo test --lib` runs
these tests as threads in ONE process (`check-cli-tests` is `cargo test`, not
nextest, so it is threads, not process-per-test), and ~47 sites in the crate
spawn subprocesses. `O_CLOEXEC` — which Rust sets on every file it opens —
closes a descriptor at **exec**, not at **fork**. Between another thread's fork
and its child's exec, that child owns a copy of every descriptor open in the
process, including this test's write handle on `idlc`. An `execve` landing
inside that window sees a writer and gets `ETXTBSY`.

Not a hypothesis — measured. A standalone repro where **every writer uses a
unique path** (so a path collision cannot contribute):

| concurrently-forking threads | execs | `ETXTBSY` |
| --- | --- | --- |
| 0 (writers fork each other's stubs) | 1600 | 92 |
| 2 | 1600 | 147 |
| 12 | 1600 | 391 |

Monotone in fork rate, and non-zero even with no dedicated noisemakers —
the writer threads' own `Command::spawn` calls are enough.

## Why this is NOT issue 0455

0455 fixed a real and different cause of the same error message: 22 hand-rolled
`temp_dir()` spellings, several of which two concurrent runs agreed on, so one
run's `remove_dir_all` truncated the stub another was exec'ing.

That fix cannot explain this occurrence. At the time of this failure the path
was **already** pid-scoped — `/tmp/nros-cli-core-tests-<pid>/<test>/idlc`, as
the traceback shows — and only one test uses that tag, so no second writer
existed, in this process or any other. Path uniqueness is necessary and not
sufficient: the descriptor is inherited by pid, not by path.

So the two issues are consecutive causes of one symptom, and 0455's landing is
why this one became visible on its own.

## Resolution

`crate::test_support::write_executable_stub(path, script)` — the one way a test
in this crate writes something it will later execute. It writes the content to a
non-executable source path (never exec'd, so its descriptor is harmless), then
runs `cp` and `chmod` as **child processes**. The only write handle on the stub
therefore lives in a process our forks do not copy from, and it is gone before
the stub is executed.

Candidates, measured on the same repro (1200 execs, 12 forking threads):

| approach | escaped `ETXTBSY` | cost |
| --- | --- | --- |
| write in-process, then exec (the defect) | 245 | — |
| **write from a child process** | **0** | one extra `cp`+`chmod` per stub |
| retry the exec on `ETXTBSY` | 0 | 141 backoffs, and the race remains |

The retry works, and is what most projects reach for; it was rejected because it
masks the race rather than removing it, and pays latency on every hit.

Regression test: `test_support::tests::executable_stub_survives_concurrent_forks`
writes and execs 40 stubs while four threads fork continuously. Verified to FAIL
against the old spelling (3 of 40 iterations hit `ETXTBSY`) and pass with the
helper.

## Scope of the class

Repo-wide sweep for "writes a file with the exec bit":

```console
$ grep -rn --include='*.rs' 'set_mode(0o7\|set_mode(0o5' packages/ scripts/
```

Two sites. The one fixed here, and
`cargo-nano-ros/src/ament_installer.rs:298` — product code that `fs::copy`s
build outputs into an ament install dir and chmods them. Same window in
principle, but nothing execs those binaries from that process (the install verb
exits first), so it is left alone; the note is here so a future "and then run
it" step knows what it would be walking into.
