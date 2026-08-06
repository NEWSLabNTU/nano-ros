---
id: 463
title: "48 tracked leaf `.cargo/config.toml` files `include` a gitignored sidecar, so every RTOS Rust leaf is unbuildable before `nros sync`"
status: open
type: bug
area: build
related: [issue-0457, issue-0272, issue-0440, rfc-0048, phase-338]
---

## Symptom

`just rust-rtos-link-check` — a named `just ci-full` step — fails on a tree that
has not run `nros sync` since issue 0457 landed:

```
  freertos talker:
error: failed to parse manifest at `examples/qemu-arm-freertos/rust/talker/Cargo.toml`
  failed to load config include `nros-managed-patch.toml` from
    `examples/qemu-arm-freertos/rust/talker/.cargo/config.toml`
  failed to read configuration file
    `examples/qemu-arm-freertos/rust/talker/.cargo/nros-managed-patch.toml`
  No such file or directory (os error 2)
error: recipe `rust-rtos-link-check` failed with exit code 101
```

It is a *manifest parse* failure, so it defeats `cargo metadata` too — not just
builds. Any tool that reads one of these leaves reports the leaf as broken.

## Root cause: the resolution rests on a premise cargo does not honour

0457 moved sync's managed `[patch.crates-io]` block out of the tracked leaf
config into a gitignored sibling, `.cargo/nros-managed-patch.toml`, reached by
an `include` entry in the tracked half. Its write-up says (archived
`0457-…md:75-77`):

> The entry is dropped when the managed set empties — cargo **ignores a missing
> `include` SILENTLY**, the failure …

Measured on cargo 1.97.1 (c980f4866 2026-06-30), that is not what cargo does. A
missing `include` target is a hard error, as the trace above shows. Dropping the
entry when the set empties is therefore load-bearing in a way the design did not
account for: the entry is *present* in the committed file, and the file it names
can never be present in a fresh clone, because `.gitignore:119` ignores it:

```
**/.cargo/nros-managed-patch.toml
```

So the tracked half asserts a file that only a per-host generator produces. This
is the same shape as the leaf-lockfile rule in CLAUDE.md — a committed artifact
naming something that does not exist in a bare clone — and it fails the same way.

## Blast radius

Measured over the tracked set, not sampled:

```
$ git ls-files '*/.cargo/config.toml' | while read f; do
    grep -q 'nros-managed-patch' "$f" && echo "$f"; done | wc -l
48

$ # …of which, sidecars actually present on this host:
0
```

48 tracked leaf configs carry the include. Zero sidecars exist anywhere in the
tree, on a host that has been building these leaves all week. Every one of those
48 leaves fails to parse until `nros sync` writes its sidecar.

## Confirmation that the include is the sole cause

Dropping a one-comment placeholder in makes the leaf parse:

```
$ printf '# probe\n' > examples/qemu-arm-freertos/rust/talker/.cargo/nros-managed-patch.toml
$ (cd examples/qemu-arm-freertos/rust/talker && cargo metadata --no-deps >/dev/null)
with empty sidecar -> exit 0
```

Nothing else about the leaf is wrong. The include is the whole failure.

## Why the gates missed it

The tree that validated 0457 had already run sync, so every sidecar existed
locally. No gate asserts the *bare-clone* property the tracked half now depends
on — which is precisely the issue-0196 rule (a gate whose coverage is narrower
than the invariant it enforces). `_require-fixtures` has an analogue for
fixtures; there is no `_require-sync` for leaves.

## Options

1. **Bootstrap the sidecar.** Commit a placeholder — but it is gitignored by
   design, and un-ignoring it re-creates the churn 0457 removed.
2. **Drop the `include` from the tracked half and have sync add it.** Restores
   0457's intent honestly: the entry exists only when the file does. Sync
   already rewrites this file, and already evicts its own stale keys, so it is
   the natural owner of the entry too. Costs: the tracked file is no longer a
   complete config, which is already true in substance.
3. **A `_require-sync` guard** on every recipe that touches a leaf, mirroring
   `_require-fixtures`, so the failure is a legible "run `nros sync`" instead of
   a cargo manifest-parse trace 4 frames deep.

(2) and (3) compose and are the recommended pair: (2) makes a fresh clone
parse, (3) makes the *absence* of the patch set fail loudly at the seam where
it matters rather than silently resolving a message crate against crates.io —
the failure mode #378 already cost us once.

Whichever lands, the gate must assert the bare-clone property directly, or this
returns the next time someone edits the leaf config layout.
