---
id: 587
title: "`check-cargo-config-tracked` calls six threadx-linux configs \"pure sync output\" and tells you to untrack them — the content it ignores is issue 0582's lesson"
status: resolved
type: bug
severity: high
area: build, testing
related: [issue-0559, issue-0582, issue-0457, issue-0463, phase-351]
---

## Symptom

`just check fast` — and therefore `just ci` — is red:

```
check-cargo-config-tracked: pure sync-output cargo config IS tracked:
  examples/threadx-linux/rust/talker/.cargo/config.toml
  examples/threadx-linux/rust/action-client/.cargo/config.toml
  examples/threadx-linux/rust/service-server/.cargo/config.toml
  examples/threadx-linux/rust/listener/.cargo/config.toml
  examples/threadx-linux/rust/service-client/.cargo/config.toml
  examples/threadx-linux/rust/action-server/.cargo/config.toml

  These hold nothing but sync's own include + [patch.crates-io] block, so
  they are recreated by `nros sync` and only churn in git. Untrack with:
      git rm --cached <path>
```

**Do not run that command.** The premise is false and the remedy destroys
documentation.

## The premise is false

The files do not hold "nothing but sync's own include + `[patch.crates-io]`
block". Each carries a hand-written comment block, and in these six it is the
whole point of the file — `examples/threadx-linux/rust/talker/.cargo/config.toml`
records why there is deliberately NO `[build] target`:

> ThreadX-Linux is a Linux-userspace simulation port … so the Entry pkg is a
> regular HOST binary — and "host" means whatever host this is. The literal
> `x86_64-unknown-linux-gnu` that used to sit here read as a host pin on an x86
> machine and as a CROSS COMPILE everywhere else, so the leaf failed to build on
> any other host.

That is issue **0582**'s finding, written down at the exact place someone would
otherwise re-add the triple. It also documents that the artifact does NOT land
in `<leaf>/target/` (the fixture build routes these rows into a shared group
dir) and which resolver tests must use instead.

`git rm --cached` on these files deletes all of that from the tree.

## Cause — the predicate scores comments as nothing

`scripts/check-cargo-config-tracked.sh`:

```sh
# Content beyond what sync writes: anything that is not blank, a comment, the
# include line, the patch table header, or an `# nros-managed` entry.
has_authored_content() {
    grep -qvE '^\s*$|^\s*#|^include = |^\[patch\.crates-io\]|# nros-managed\s*$' "$1"
}
```

`^\s*#` is excluded, so a file whose ONLY authored content is comments reads as
pure sync output. For most leaves that is right — a stray comment above a
generated patch table is not worth tracking. For these six it inverts the rule
the tree actually follows, which CLAUDE.md states as: the authored half "stays
tracked because a clone cannot regenerate it".

A comment IS content a clone cannot regenerate. `nros sync` will not rewrite
that paragraph; it will write the include and the patch rows and nothing else.

## It also contradicts a fix that landed deliberately

Issue **0559** committed these files ON PURPOSE. Its record: "39 leaves commit
both; `threadx-linux/rust/talker` committed neither", and the fix was to commit
them so sync stops rewriting a tracked file on every build and so a fresh clone
can link the leaf. `13878a807` (#582) then edited them by hand.

So one gate now demands the removal of what another fix deliberately added.
Whichever way this is resolved, both cannot stand.

## Suggested direction

Count a comment as authored content when it is more than a trivial header —
or, more simply, treat any file containing a comment that `nros sync` does not
itself emit as authored. Sync's own output is fixed and known, so "content sync
would not write" is decidable without a heuristic about comment length.

The narrower alternative — allowlisting these six paths — is worse: it is the
hand-kept list this repo keeps paying for, and the next hand-authored config
would hit the same wall.

## Not investigated

Whether the other ~39 leaves that commit a config would newly FAIL or PASS under
a corrected predicate. That number decides whether this is a six-file fix or a
convention change, and it should be measured before either.


## RESOLVED 2026-08-15

`has_authored_content()` no longer excludes comments wholesale. It excludes only
the decor `nros sync` itself emits — the `# === BEGIN/END nros-managed
[patch.crates-io] ===` pair and the trailing `# nros-managed` on each patch row
(`nros-cli-core/src/cmd/ws.rs`). Any other comment came from a human, so the
predicate needs no judgement about which comments "look important".

Verified both directions rather than just re-running the gate:

* a synthesised config containing ONLY sync's include, patch table and decor
  still scores as pure — the gate has not been disabled;
* `examples/threadx-linux/rust/talker/.cargo/config.toml` now scores as
  authored, so the six stop being reported and issue 0582's paragraph stays in
  the tree.

**The open question above is answered, and it changes the shape of the finding.**
All **74** tracked leaf configs score "pure" under the OLD predicate — not six.
The other 68 were never reported only because the gate has a second condition:
it flags a tracked pure config only when it does not `includes_committed_projection`.
The threadx-linux six lost their board projection in `13878a807` (#582), which
is what exposed them.

So this was never a six-file problem: the predicate was wrong for every config
in the tree, and a projection exemption was masking it. That also means the fix
could not have been an allowlist — 68 files were one upstream edit away from the
same false positive.
