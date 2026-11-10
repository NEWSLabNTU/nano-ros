---
id: 2434
title: "`nros dev migrate-bringup` refuses a standalone package with no system.toml, so such a package is migrated by hand"
status: open
type: tech-debt
severity: low
area: [cli, examples]
related: [rfc-0121, rfc-0122, rfc-0071, phase-535]
found: 2026-11-11
---

Found during phase-535 W6 (the linux Rust owned-entry demos) on
`examples/linux/rust/logging`.

## Measured

2026-11-11, on `main` after #3512. The logging example before its migration
(`git archive origin/main examples/linux/rust/logging`), run through the
migrator:

```
$ nros dev migrate-bringup <copy>/examples/linux/rust/logging
Error: <copy>/examples/linux/rust/logging has no system.toml: there is nothing to migrate
```

(`nros-cli-core/src/cmd/migrate_bringup.rs`.)

## Why it matters

RFC-0122 D3 makes every example kept in its own directory a one-package
workspace. A package with no `system.toml` still needs a bringup when it
becomes one: an image, a board and its owned entry (RFC-0071 D19). The
logging demo drives the `nros-log` sink and opens no session, so it never
had a `system.toml`. The migrator takes "no system.toml" to mean "nothing
to migrate". In fact the bringup is missing and could be synthesised.

So phase-535 W6 wrote `src/native_logging_bringup/` (a `package.xml` and a
`system.toml` with one `linux` image naming the owned entry `native_logging`)
and moved the package by hand. Everything the migrator does for a package
with a `system.toml` had to be done by hand: moving the package under
`src/`, the `.colcon_workspace` marker, the fixture-row rewrite and the
README.

## Fix shape

When `system.toml` is absent, either:

- synthesise a minimal bringup: `[system] name` from the package, the
  default board of the package's `deploy` list, and one image naming the
  package as its owned entry when the package has a hand-written `main`; or
- refuse with a message that names the missing input and the one-line
  `system.toml` that would make the package migratable, instead of
  "nothing to migrate", which reads as "already done".

## Acceptance

`nros dev migrate-bringup` on a package with a `[[bin]]` and no
`system.toml` either produces a workspace that `nros build` builds, or
refuses and says which file to write.
