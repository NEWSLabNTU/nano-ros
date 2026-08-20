---
id: 723
title: "`platform-sink` cannot gate a LINK requirement, because `cargo --workspace` unifies features while linking stays per-binary"
status: open
type: bug
severity: high
area: build
related: [issue-0714, issue-0710, issue-0708]
---

# 0723 — `just check` is still red on main after 0714

Issue 0714 put 0710's sink auto-install behind a new `nros-log/platform-sink`
feature, reasoning (in `nros-board-esp32-qemu`'s manifest):

> Cargo unifies the feature into any image holding this crate; consumers with no
> port (host tools, the test harness) never get the link requirement.

The first clause is right and the second does not follow. Cargo unifies features
across **every member selected by one build**, and a `--workspace` build selects
them all — so the feature is on for `nros-log` everywhere, while *linking*
remains per-binary. A test binary that never links a platform port still gets
`nros-log` compiled with `platform-sink`.

Still failing, on `888aa0135`, in `just check-workspace-features`:

```
cargo test --no-run --workspace --exclude nros-c --no-default-features --quiet

rust-lld: error: undefined symbol: nros_platform_log_write
  >>> referenced by sinks.rs:72 …(<nros_log::sinks::PlatformSink as nros_log::LogSink>::log)…
rust-lld: error: undefined symbol: nros_platform_log_flush
```

Seven `nros-rmw-cffi` targets: `two_backends`, `ping_session`,
`server_available`, `try_recv_sequence`, `process_in_place`, `publish_streamed`,
and the lib test.

## The reproduction that separates the two readings

```
$ cargo test --no-run -p nros-rmw-cffi --no-default-features --quiet   # solo
OK
$ cargo test --no-run --workspace --exclude nros-c --no-default-features --quiet
rust-lld: error: undefined symbol: nros_platform_log_write
```

Same crate, same features requested, opposite results — which is the signature of
unification rather than of anything in `nros-rmw-cffi`.

The member that turns it on is `nros-board-linux`, a root workspace member that
requests it unconditionally:

```toml
nros-log = { version = "0.5.0", path = "../../core/nros-log", default-features = false,
             features = ["alloc", "platform-sink"] }
```

`cargo tree -p nros-log -e features --workspace` shows `nros-log feature
"platform-sink"` reached from it. Eight board crates and four platform crates
request the feature; `nros-board-linux` is the one that is a root member, so it
is the one that reaches this build.

## Why a feature is the wrong instrument here

A cargo feature is a property of a **compilation**; an undefined symbol is a
property of a **link**. Unification means the first is shared across a workspace
build and the second is not, so no feature can express "only the binaries that
also link a port". `nros-log`'s own `platform-clock` comment gets this right by
accident — it is opt-in per consumer, and no workspace member turns it on
unconditionally, so nothing forces it on the graph.

## Options

1. **`nros-board-linux` stops requesting it unconditionally** — put its
   `platform-sink` behind a feature of its own that only an image enables. Keeps
   0714's shape; the question is whether every board can do this, since a board
   IS the console and 0714 chose unconditional deliberately.
2. **Weak/no-op fallback definitions in `nros-log`.** Makes the link succeed with
   or without a port, so unification stops mattering. Weak symbols across the
   Rust/C seam are their own hazard here (`nros-c`'s FORCE_LINK class, issues
   0155/0163).
3. **Exclude the board crates from the no-default-features smoke.** Narrowest,
   and it removes the coverage that found this.

This is 0714's design to finish, so it is filed rather than patched.

## Note for whoever takes it

0714 is archived as resolved. Its own gate, `check-board-log-sink.py`, passes —
it checks that boards request the feature, which they do. The gate and the red
are not in contradiction; the gate simply asks a different question than the one
that fails.
