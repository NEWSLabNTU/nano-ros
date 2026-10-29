---
id: 2124
title: "A `[[domain]]` no bridge opens was accepted when it named no file, and silently read by nothing"
status: resolved
type: limitation
area: [orchestration, cli, rmw]
severity: low
found: 2026-10-29
resolved: 2026-10-29
related: [issue-2062]
---

## What

RFC-0109 D6 originally called `[[domain]]` "the only spelling of a session".
Phase-514 W3b's amendment made it the spelling of a *bridged* session instead,
and spelled the image's default session `[system] rmw_config` "until a lone
domain becomes the default session". It refused a `[[domain]] config` that no
`[[bridge]]` opens (`SystemToml::check_rmw_files`), naming `[system] rmw_config`.

When filed, this issue said a lone domain's `config` "resolves without error
and is never used". That was wrong: the `config` case was already refused. The
real hole was narrower. A lone domain that named no file was accepted:

```toml
[system]
rmw = "zenoh"
domain_id = 0

[[domain]]
name = "main"
rmw = "cyclonedds"
id = 7
```

Its `rmw` and `id` reached no session. The image opened zenoh on domain 0.
The domain's backend was still added to the image's backend set
(`facade::image_backends`), so Cyclone got linked into an image that opens no
session on it. A lone domain carrying only a `locator` was accepted the same
way.

## Decision

A lone domain does **not** become the default session. The launch resolver
(rlm's `parse_system_config`) reads only `[system] rmw` / `domain_id`. A
second spelling of the default session would make the two `system.toml`
readers disagree, which is the issue 0293 class that
`tests/system_toml_readers_agree.rs` guards, and it would need a matching
change in `play_launch`. No tracked `system.toml` declares a lone domain; the
only two that use `[[domain]]` are the bridge examples.

## Resolution

`check_rmw_files` now refuses **every** `[[domain]]` that no `[[bridge]]`
opens, whatever it states (`config`, `locator`, or only `rmw` / `id`). The
remedy names the default session's homes: `[system] rmw`, `[system]
domain_id`, `[system] rmw_config` (or `[system] locator`), each overridable
per `[image.<id>]`. The check runs at sync, under `fold_os_config`, on every
road.

RFC-0109 D6 and its W3b amendment item 2 now say this. The RFC's
platform/use-case examples that showed a lone `[[domain]] main` as an image's
session (Linux, Zephyr on a real board, U8, U9) are rewritten in the
`[system]` / `[image]` spelling.

Test: `orchestration::rmw_config::tests::a_domain_file_is_checked_against_its_own_backend_and_id`
gains the no-file case (`rmw` + `id` only) and the locator-only case. Both
failed before the change.
