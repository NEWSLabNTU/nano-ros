---
id: 1337
title: "The prereq scan's package.xml walk descends into every out-of-source
  build tree, because it prunes `build` and `target-*` but not `build-*`"
status: resolved
type: bug
area: cli, orchestration, performance
severity: high
resolved_in: "fix(#1337): one pruning predicate for both package.xml walks, and it knows build-"
related: [issue-0363]
---

## What happens

`nros image-facts` on a workspace that keeps its build trees beside `src/`
spends minutes in uninterruptible sleep before printing anything. So does
anything else that resolves a workspace, because they share the scan.

Measured on Autoware Safety Island, whose tree had accumulated 85 `build-*`
directories from repeated `west build -d build-<name>` and
`cmake -B build-<name>` runs:

```
$ strace -f -e trace=openat nros image-facts --for-entry zephyr_entry --cmake
total openat                    367,540
under build-*                   362,782   (98.7%)
of those, O_DIRECTORY           362,781
regular files actually read           1
```

362,781 directory opens to read one file. The kernel counters for the same
process:

```
rchar:        27,338      # 27 KB returned to userspace
syscr:            54      # in 54 read syscalls
read_bytes:  242 MB       # physical reads -> 9,283x amplification
```

Nearly all of it is filesystem metadata. On btrfs over a 7200 RPM disk that
is minutes blocked in `read_extent_buffer_pages`, and the disk sits at 98%
utilisation with 30 ms read latency while it runs.

## Why

`orchestration/prereq_resolve.rs` has two `package.xml` walks --
`declared_depends` and `package_xml_files` -- and the pruning rule was
written out separately in each:

```rust
if !matches!(
    name.as_str(),
    "build" | "target" | ".git" | "external" | "third-party" | "node_modules"
) && !name.starts_with("target-")
```

It prunes the `target-` PREFIX and the exact name `build`, but not the
`build-` prefix. So `build` is skipped and `build-zephyr`, `build-board`,
`build-heap` and their 82 siblings are walked in full.

The CLI has two other `package.xml` walkers and both get this right:

- `nros-pkg-index/src/lib.rs` -- `if file_name.starts_with("build-")`
- `builder/discover.rs` -- `|| name.starts_with("build-")`

so the workspace this scan sees is not the workspace the index sees. Beyond
cost that is a correctness gap: a stale `package.xml` left inside a build
tree contributes its `<depend>` names to `declared_depends`, and no other
component of the CLI agrees that package is present.

Two copies of a rule is why one of them drifted.

## Fix

One `is_pruned_dir(name, path)` predicate, used by both walks. It prunes the
`build-` prefix alongside `target-`, adds `.cargo` and `__pycache__` to match
the pkg-index skip list, and honours the ament ignore markers
(`COLCON_IGNORE`, `AMENT_IGNORE`, `NROS_IGNORE`, `.nros-ignore`) that the
sibling walkers already honour -- so the three walkers agree about what is in
the workspace.

Verified: on the island tree the same `image-facts` invocation drops from
362,782 opens under `build-*` to zero, and two tests hold it -- one puts a
stale `package.xml` inside `build-zephyr/` and asserts neither the file nor
its `<depend>` is found, one asserts a `COLCON_IGNORE` directory is pruned.
