---
id: 935
title: "Every `.launch.py` ABORTS through the shipped resolver — `exec_file`'s
  inputs and outputs travel by a thread-local the dlopen split duplicated"
status: open
type: bug
area: tooling, cli
related: [0914, 0897, 0915, phase-332]
---

## Symptom

The shipped `nros-launch-resolve` cannot resolve ANY Python launch file. Not a
clean error — an abort:

    $ nros-launch-resolve <anything>.launch.py -o m.yaml
    thread '<unnamed>' panicked at crates/play_launch_parser/src/bridge.rs:367:
      No LaunchContext set - Python execution must be called through LaunchTraverser
    --- PyO3 is resuming a panic after fetching a PanicException from Python. ---
    panic in a function that cannot unwind
    thread caused non-unwinding panic. aborting.

Measured on all three in-tree fixtures, including `test_no_import.launch.py`,
which imports nothing — so it is not about ROS packages being on `sys.path`.

XML is unaffected: `$(eval …)` works, and `multihost_partition_bake` passes and
fails correctly when `libplay_launch_parser_pyexec.so` is removed.

## Cause

`exec_file` communicates through a **process-wide thread-local that is no longer
process-wide.** The C ABI carries `{op, arg}` and nothing else, and both sides
say so in their own comments:

`c_abi.rs`, on the response value:

> Empty for `exec_file`, whose effects land in the parser's thread-local launch
> context, not here.

`executor.rs`, before running the file:

> Launch configurations are already in the thread-local LaunchContext
> (set by `execute_python_file()` before calling this executor)

`execute_python_file()` runs in the BINARY. The executor runs in the `.so`. And
**both statically link `play_launch_parser`** — `with_launch_context` is present
in each and exported by neither:

    nros-launch-resolve              internal=2  exported=0
    libplay_launch_parser_pyexec.so  internal=12 exported=0

So there are two `CURRENT_LAUNCH_CONTEXT` thread-locals. The binary sets its
own; Python runs inside the `.so` and calls that copy, which is `None`. Same
thread, different variable.

`$(eval …)` survives because its request carries the expression and its response
carries the result — that path never touches the shared state. `exec_file`
carries neither direction, which is exactly why it is the one that broke.

## Why every test says it is fine

`play_launch_parser` has a **dev-dependency on `pyexec`**, so its own
`.launch.py` tests link the executor IN-PROCESS: one copy of the crate, one
thread-local, green. The shipped configuration `dlopen`s a second copy.

That is issue 0914's thesis — "a test that builds its own Python half proves the
mechanism and not the packaging" — with a worse consequence than 0914 described.
0914 was wrong that nothing covers the shipped pair (`multihost_partition_bake`
does, by path, and fails when the `.so` is removed); it was right that the
coverage is shaped so this class hides in it, and the `.launch.py` half is not
merely untested but broken.

## Fix

`exec_file` needs both directions in the protocol, since the shared state it was
written against does not exist:

1. **Request carries the launch configurations**, so the `.so` can establish its
   OWN `LaunchContextGuard` for the duration of execution.
2. **Response carries the captures** (nodes, containers, load_nodes, global
   params), which the binary merges into its context — today they are written to
   the `.so`'s copy and discarded when it returns.

Rejected alternatives, and why:

* **Export the bridge symbols from the binary** so the `.so` resolves them at
  load. Makes the ABI the whole Rust surface rather than two C functions, and
  reintroduces exactly the coupling issue 0897 removed.
* **Make `play_launch_parser` a shared library** so one copy exists. Same
  coupling, plus a second `.so` to ship and version.

Both are worse than serialising two more fields.

## Also worth fixing while there

A `panic!` inside a PyO3 callback becomes a PanicException and then a
non-unwinding abort with a 27-frame backtrace. `with_launch_context` should
return a `Result` a caller can report, so a missing context reads as an error
about a launch file rather than a crash in the tool.

## Verifying

A test that resolves a `.launch.py` through the INSTALLED binary by path — the
shape issue 0914 asked for and the fixtures already exist. It must SKIP (not
pass) where no interpreter is usable, or it re-creates the hole one level down.
Note `multihost_partition_bake` currently has no Python probe, so on a
Python-less host it fails rather than skipping.
