---
id: 249
title: "cargo-nano-ros scaffold bakes maintainer TODO@todo.com into generated package.xml"
status: resolved
type: polish
severity: low
area: cli
---

## Finding (release-prep audit 2026-07-24)

`packages/cli/cargo-nano-ros/src/scaffold.rs` (lines ~75, 175, 336, 495):
every scaffolded `package.xml` ships

```xml
<maintainer email="TODO@todo.com">TODO</maintainer>
```

User-visible polish: `nros new` output carries a placeholder the user must
notice and edit; colcon/bloom tooling surfaces the maintainer field.

## Fix

Take maintainer name/email from git config (`user.name`/`user.email`) at
scaffold time, falling back to a clearly-instructional placeholder
(`<maintainer email="you@example.com">Your Name</maintainer>` plus a
scaffold-time stdout note). One helper, four call sites.

## Resolution (2026-07-24)

Landed (`7004c50fd`): maintainer sourced from `git config user.name`/
`user.email`, instructional `you@example.com` fallback. One helper, all four
package.xml sites.
