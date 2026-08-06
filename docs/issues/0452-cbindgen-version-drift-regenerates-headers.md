---
id: 452
title: "Embedded builds regenerate the cbindgen headers with different output, dirtying tracked files"
status: open
type: bug
area: build
related: [phase-338]
---

## Symptom

Running an embedded lane (`just nuttx build-examples`, `fixtures-build.sh nuttx
cpp`, …) leaves two TRACKED, generated headers modified:

```
 M packages/api/nros-c/include/nros/nros_generated.h
 M packages/api/nros-cpp/include/nros/nros_cpp_ffi.h
```

The diff is not a content change — it is a cbindgen VERSION difference. The
committed headers guard C23 enum bases:

```c
 enum nros_cpp_sched_class_t
-#if defined(__cplusplus) || __STDC_VERSION__ >= 202311L
+#ifdef __cplusplus
   : uint8_t
-#endif // defined(__cplusplus) || __STDC_VERSION__ >= 202311L
+#endif // __cplusplus
```

The embedded build rewrites them with the older, narrower guard — about 36 lines
across the two files, every time.

## Why it matters

* A build silently dirties the worktree, so `git status` after any embedded lane
  shows changes nobody made.
* Committing it **reverts an upstream improvement** — the C23 guard is the newer
  output. During phase-338 this had to be reverted twice by hand before pushing;
  a less careful `git add -u` would have landed it.
* It is the "generated output drifts between tool versions" hazard the repo
  already pins for elsewhere: `.clang-format-version` + `just setup-clang-format`
  exist precisely because "clang-format output drifts between major versions …
  an unpinned PATH `clang-format` produces spurious diffs / `check-*-fmt`
  failures across machines". cbindgen has the same property and no such pin.

## Fix

Mirror the clang-format treatment:

* pin the cbindgen version the headers are generated with (a `.cbindgen-version`
  SSoT plus a `just setup-cbindgen` that provisions it, or a locked
  `cbindgen-cli` the build scripts invoke by absolute path);
* have the build scripts use the pinned binary rather than whatever `cargo`
  resolves in that graph;
* consider a `check-cbindgen-headers` gate in the same shape as
  `check-abi-bindings`, which already guards the OTHER direction (committed
  bindgen output going stale against the C headers).

Note the precedent is exact: `scripts/gen-abi-bindings.sh` already pins
`bindgen-cli 0.72.1` for the C→Rust direction. The Rust→C direction has no
equivalent.
