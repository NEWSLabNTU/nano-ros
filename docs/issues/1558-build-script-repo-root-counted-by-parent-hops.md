---
id: 1558
title: "Seven build scripts derive the repository root by counting `.parent()`
  hops, and the class has been fixed three times by re-counting them"
status: open
type: tech-debt
area: build
severity: medium
found: 2026-09-29
related: [0365, 1280, 1336, 1527, 1558, 1560, phase-471, RFC-0101]
---

## What this is

`nros_build_paths::repo_root()` finds the repository root by walking up for the
`nros-sdk-index.toml` marker. It is depth-independent, so a crate that moves
keeps resolving. Seven sites do not use it: they count `.parent()` hops from
`CARGO_MANIFEST_DIR`, or join a literal `"../../.."`, which encodes the crate's
CURRENT depth into a path that is read at build time and never type-checked.

RFC-0101 D3 forbids it. This issue is the migration.

## The sites (measured 2026-09-29)

| file:line | what it reaches | shape |
| --- | --- | --- |
| `packages/rmw/xrce/nros-rmw-xrce-cffi/build.rs:87-92` | micro-XRCE-DDS-Client, micro-CDR, `xrce-sources.txt`, `xrce-config.txt`, three first-party include dirs | four chained `.parent()` |
| `packages/boards/nros-board-mps2-an385-freertos/build.rs:143-147` | `nros-c` headers | two chained `.parent()` |
| `packages/boards/nros-board-mps3-an536-freertos/build.rs:86-89` | `nros-c` headers | two chained `.parent()` |
| `packages/boards/nros-board-s32z270-freertos/build.rs:86-89` | `nros-c` headers | two chained `.parent()` |
| `packages/boards/nros-board-threadx-linux/build.rs:45-49` | ThreadX, NetX Duo and `nros-c` defaults | three chained `.parent()` |
| `packages/boards/nros-board-common/src/threadx_qemu_riscv64_build.rs:52-56` | ThreadX, NetX Duo, `virtio-net-netx`, `nros-c` defaults | three chained `.parent()` |
| `packages/rmw/zenoh/nros-zpico-build/src/runner.rs:1357` | the platform-descriptor search root | `manifest_dir.join("../../../..")` |

The table lists the deepest walk per file. Two of them carry a shallower
sibling walk as well (`nros-board-{mps3-an536,s32z270}-freertos/build.rs:42-45`,
one hop to `nros-board-freertos/config`), which is the same shape at a depth
where it is less likely to move.

Two further sites are the rule's third spelling — a CWD-relative literal, which
works only because a build script's cwd is its crate root:
`packages/rmw/cffi/build.rs:369,401` (`../../core/nros-rmw-abi/include`) and
`packages/platform/nros-platform-cffi/build.rs:29-31,55,64-66,69`
(`../nros-platform-api/include`, `../nros-platform-posix/src/*.c`).

## Why it matters — three defects, each fixed by re-counting

The class is not hypothetical. Every fix so far adjusted the hop count, which
is why it keeps recurring:

1. **`nros-rmw-xrce-cffi`, phase-321 W2.d.** The crate moved from
   `packages/xrce/` to `packages/rmw/xrce/`. Three parents became four, and in
   between *"every vendored path came out doubled
   (`<repo>/packages/packages/rmw/xrce/...`)"*. The comment left at the site
   states the reason a rule is needed better than this issue can: *"A
   `.parent()` chain is a relative path that no grep for `../` can find — only a
   build does."*
2. **The three FreeRTOS boards, issue 0365.** `nros-c` moved to
   `packages/api/nros-c` in phase-321 W2.e and *"this join was left at the old
   `core/nros-c`, so the TU could not find the header."* The remedy added an
   `assert!` on a known file — a per-site tripwire, not a fix for the class.
3. **`nros-zpico-build`, phase-400 W1.** `manifest_dir.join("../../../../config")`
   kept resolving after the platform descriptors moved out of that directory, so
   *"every platform silently fell back to builtins. A wrong image, no
   diagnostic."* This is the worst of the three: the walk did not break, it
   found a directory that no longer held what was wanted.

Failure mode 3 is the argument for a rule rather than more asserts. A counted
walk that lands somewhere real and wrong produces no error at all.

## Remedy

Replace each with `nros_build_paths::repo_root()` (or `try_repo_root()` where an
out-of-tree consumer must keep working). Where the crate does not yet depend on
`nros-build-paths`, it is a zero-dependency host-only crate, so the edge is
cheap — but check `packages/rmw/cffi` and `nros-platform-cffi` first, since a
new build-dependency there reaches further than it looks.

**Do not migrate the Zephyr leaf shims or `packages/reference/stm32f4-porting/*`
even if they grow such a walk** — a copy-out leaf (RFC-0026) may not depend on a
crate that exists only in this checkout. They have none today.

## Acceptance

Each affected crate still builds and the resolved paths are unchanged. A gate is
the follow-on, not part of this: the shape to refuse is a `.parent()` chain or a
`"../"`-joined literal in a build script whose purpose is to reach the repo
root, and it must not fire on a build script reaching a *sibling file inside its
own crate*. Both halves need measuring before a gate is worth writing —
`check-inherited-checkout-paths` is the gate this would extend, and phase-471 W3
is its build-script half.

## Related

* RFC-0101 D3 — the rule, and why `repo_root()`'s marker walk is the form of it.
* Issue 1560 — the other half of D3 (a raw `env::var` where the shared resolver
  belongs). Same rule, different clause.
* Issue 1336 — nothing may MODEL git's layout. Same shape one directory over.
