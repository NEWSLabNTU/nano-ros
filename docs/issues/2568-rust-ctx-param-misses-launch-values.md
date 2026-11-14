---
id: 2568
title: "A Rust node's `ctx.param()` returns the default, not the launch file's value, and the census reports the read as `param-phantom`"
status: open
type: bug
area: [api, cli, census]
severity: medium
found: 2026-11-14
related: [phase-478]
---

GitHub: NEWSLabNTU/nano-ros#3860

**Home: [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)**.
Seen on nano-ros 0.6.0-nros8.

## What happens

A Rust node's `ctx.param()` returns the default, not the launch value. Both
`<param from="file.param.yaml"/>` and an inline `<param name=... value=.../>`
fail, although the resolved model shows the value.

The census also reports the read as `param-phantom`; the L4 scaffold waives it
in `system.toml` for now.

## Direction

The launch value reaches `ctx.param()`, and the census sees the read (so the
waiver can go).
