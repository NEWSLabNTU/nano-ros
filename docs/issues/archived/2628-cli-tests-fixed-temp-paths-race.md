---
id: 2628
title: "Two `just check cli-tests` runs on one host fail each other: six CLI tests used fixed paths under the shared temp dir"
status: resolved
type: bug
severity: low
area: [cli, testing]
found: 2026-11-16
resolved: 2026-11-16
related: [0455]
---

## What happened

Two `just check cli-tests` runs started at once on one host, from two
worktrees, and both failed in `nros-build`: one with
`board_framework::tests::resolves_a_patched_version_dependency`, the other
with `resolves_a_target_scoped_board` and
`a_board_without_a_framework_key_yields_none`. The panics were
`board resolves` and an `unwrap()` on `None`. Nothing in either branch
touched `nros-build`.

## Cause

The five `board_framework` tests each wrote a fixed directory,
`$TMPDIR/nros_bf_path`, `nros_bf_patch`, `nros_bf_target`, `nros_bf_none` and
`nros_bf_indirect`, and began by deleting it. `$TMPDIR` is shared by every
process on the host, so one run removed the other's tree mid-test.
`nros-cli-core`'s `new_node.rs` test helper had the same shape
(`nros-new-node-{tag}`, no pid).

Issue 0455 fixed this class inside `nros-cli-core` and gave the crate one
helper, `test_support::scratch_dir`, with a sweep command in its module
docs. The sweep was not run over `nros-build`, a separate crate, and the
`new_node.rs` helper was written after it.

The gate's host lock serialises `just ci gate` only, so check-only runs such
as `just check cli-tests` can still overlap on one host. With unique paths
that is no longer a problem for these tests.

## Fix

- `board_framework`'s five tests take a `tempfile::tempdir()` each (already a
  dev-dependency of `nros-build`), removed when the test ends.
- `new_node.rs` uses `crate::test_support::scratch_dir`.

Sweep: every `temp_dir()` in the CLI crates whose next four lines name
neither the pid nor `tempfile`.

```python
import subprocess
out = subprocess.run(["git", "grep", "-n", "temp_dir()", "--", "packages/cli"],
                     capture_output=True, text=True).stdout
for line in out.splitlines():
    f, n, _ = line.split(":", 2)
    n = int(n)
    window = "".join(open(f).read().splitlines()[n - 1:n + 4])
    if "process::id()" not in window and "tempfile" not in window:
        print(f"{f}:{n}")
```

After the fix it prints two lines, both comments: `test_support.rs`'s module
docs and the `lib.rs` note pointing at them.
