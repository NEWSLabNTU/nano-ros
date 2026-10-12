---
id: 1257
title: "`packages/api/nros` does not compile under `-D warnings`: two `declare_parameter` results dropped, red on main since 2026-09-09"
status: resolved
area: api, params, ci
severity: medium
found: 2026-09-10
resolved: 2026-10-12
related: [1203, 0793, phase-427]
---

# The red

`just check required-features-tests` fails to COMPILE `nros` on `origin/main`:

```
error: unused return value of `Executor::<'s>::declare_parameter` that must be used
    --> packages/api/nros/src/node_runtime.rs:1243:13
error: unused return value of `Executor::<'s>::declare_parameter` that must be used
    --> packages/api/nros/src/node_runtime.rs:1673:21
     = note: `-D unused-must-use` implied by `-D warnings`
```

Both sites drop a `Result` from `declare_parameter`, so a parameter whose
declaration is REFUSED — the pool is full, the name is already taken — is
declared as far as the caller is concerned and absent as far as the store is
concerned. `#[must_use]` is doing exactly its job here; the fix is to decide what
each site should do with the refusal, not to `let _ =` it. Issue 1203 is the
neighbouring defect in the same store (`ParameterBuilder` declaring straight
into the executor), so the two should be read together.

Introduced by `ea96be9fb` (phase-427 W11, "migrate every `spin_blocking` call
site to `spin`", 2026-09-09), which is on `main`.

# Why it landed, and why it is still there

The lane that compiles this code is `required-features-tests`, behind
`required-features` — no merge-gating event reaches it (`check-build` is
`schedule`/`workflow_dispatch` only since phase-396 W1). So the push gates and
the merge queue are both green over a crate that does not compile under the
flags `just check` uses. It is the issue-0652/0612/0667 class one lane over: a
target no lane enables reads as coverage.

A local `just ci gate` DOES reach it, which is how this was found — from an
unrelated branch (issue 1228's), whose diff touches no file in
`packages/api`. So every agent running the documented pre-push tier now triages
somebody else's red first, which is the signal-capacity problem CLAUDE.md
describes for uniformly-red lanes.

# Verify

```
git stash list          # on a clean checkout of origin/main
just check required-features-tests
```

Both errors reproduce with no local changes.

## Resolution (2026-10-12)

Both sites were fixed by `f316429529` (issue 1272): each launch parameter is
now declared on its own node through `declare_parameter_on`, and a refusal
that leaves the parameter absent logs and returns `NodeDeclError::Runtime`
(`node_runtime.rs` 1390 and 1881). It no longer drops the result.

That fix moved the call to a method the guard did not cover:
`declare_parameter` was `#[must_use]`, and `declare_parameter_on` was not.
`declare_parameter_on` is now `#[must_use]` too. Every in-tree caller already
uses the result, so no call site changed.

Measured (`cargo check` of `nros` with `std,alloc,rmw-cffi,param-services` and
`RUSTFLAGS=-D warnings`; `node_runtime` is gated on `rmw-cffi`):

- tree as fixed: rc=0;
- result dropped at the 1390 site, attribute present: rc=101,
  `unused return value of Executor::declare_parameter_on that must be used`;
- result dropped and attribute removed (the guard as it was): rc=0, so the gap
  was real.

`just check required-features-tests`, the lane this issue was red in: rc=0,
38/38.
