---
id: 953
title: "A panic in the Python half crosses `extern \"C\"` and ABORTS the resolver —
  0897 removed the loader abort and left this one"
status: open
type: bug
area: tooling
related: [0897, 0915, 0914]
---

## What happens

Resolving a `.launch.py` with the shipped `nros-launch-resolve` does not fail —
it **aborts and dumps core**:

```
$ nros-launch-resolve .../tests/fixtures/launch/test_simple_python.launch.py
thread '<unnamed>' panicked at crates/play_launch_parser/src/bridge.rs:367:10:
No LaunchContext set - Python execution must be called through LaunchTraverser
--- PyO3 is resuming a panic after fetching a PanicException from Python. ---
pyo3_runtime.PanicException: No LaunchContext set …
thread '<unnamed>' panicked at library/core/src/panicking.rs:225:5:
panic in a function that cannot unwind
…
thread caused non-unwinding panic. aborting.
Segmentation fault (core dumped)
```

The first panic is arguably a misuse — that fixture belongs to the parser's own
harness and expects to be driven through `LaunchTraverser`. **The abort is not.**
A caller passing a file the resolver cannot handle should get a message.

## Why it aborts rather than returns

`pyexec`'s C entry point has no panic guard:

```rust
#[unsafe(no_mangle)]
pub unsafe extern "C" fn play_launch_py_call(req: *const c_char) -> *mut c_char {
```

`grep -c catch_unwind c_abi.rs` → **0**. A panic unwinding out of an
`extern "C"` function is undefined behaviour, and rustc's guard turns it into an
immediate abort. So *any* panic anywhere in the Python half — the parser, a
`#[pyclass]` mock, or a `PanicException` PyO3 resumes from Python — takes the
whole process down with no diagnostic from us.

The function is otherwise careful about exactly this: it returns a structured
error for a null request, for non-UTF-8, and even for a NUL byte in its own
response ("returning null on it would be an unannounced second failure mode").
Panics are the one path out that it does not cover.

## Why this matters beyond one fixture

This is the failure mode [issue 0897](archived/0897-resolver-libpython-runtime-discovery.md)
was filed to remove. Its `pyload` header states the contract:

> **What the caller gets** — A `Result`, never an abort.

0897 delivered that for the *loader*: a missing or mismatched `libpython` is now
a caught error naming the remedy. The `.launch.py` execution path still has an
abort in it, from a different cause. One artifact, two ways to die, one of them
now fixed — which is easy to mistake for the whole thing being fixed.

It also defeats the degradation 0897 built. The point of dispatching by
extension was that an unusable Python path fails *while naming the file*. An
abort names nothing, returns no exit status the caller can interpret, and in
`nros sync` surfaces as a child that died on a signal.

## Fix

Wrap the body in `std::panic::catch_unwind` and convert a panic into the
`Response { ok: false, error }` the ABI already defines. The payload's message
is recoverable via `downcast_ref::<String>` / `&str`, so the report can name the
original panic rather than "panicked".

Two details worth getting right:

* `AssertUnwindSafe` will be needed around the closure; the alternative
  (making every captured type `UnwindSafe`) is not worth it at a boundary that
  already deals in JSON strings.
* `play_launch_py_abi_version` should bump if the error shape changes, since
  `pyload` checks it.

## Scope note for 0897

0897's Q4 section says "zero tracked `.launch.py` files exist in nano-ros …
so the `.py` path is exercised by no fixture". The first half is true —
`git ls-files` does not descend into submodules — but the conclusion is too
strong: `play_launch` carries several `.launch.py` fixtures in its own tests.
The abi3 measurement there covered the `$(eval)` path only, which is what the
`nano-ros` workspaces exercise; it says nothing about `.launch.py` execution
cost. Corrected in the same change that files this.
