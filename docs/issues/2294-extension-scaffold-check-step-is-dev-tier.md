---
id: 2294
title: "The runner, command and board scaffolds name `nros dev … check` and `just check fast` as the next step, so a user has no check step for a package they made"
status: open
type: enhancement
area: cli, ux, scaffold
related: [rfc-0117, rfc-0119, phase-527, phase-537, 2291]
found: 2026-11-06
---

**Home: none yet.** Reported by the phase-537 W12 book session with an
in-tree CLI. No RFC or phase plans a user-tier check verb (see below), so
this issue proposes one.

## Symptom

After `nros new runner <name>` or `nros new command <name>`, the scaffold
prints:

```
next: nros build --packages-select <name> && nros dev runner check <name>
```

The generated `README.md` says the same (`nros dev runner check <name>` /
`nros dev command check <name>`). After `nros board new` the last next-step
line is `just check fast   # the scaffold's acceptance …`.

Neither is a user step. `just check fast` is a recipe in the nano-ros
checkout and does not exist for a user's workspace. `nros dev` is the
contributor and build-system tier. It is hidden from `nros --help`
(RFC-0117 D1), and a user is never told to go there. A user who authored
a runner or a board is left with no check.

## Code site

- `packages/cli/nros-cli-core/src/cmd/new_extension.rs`: the
  `eprintln!("… next: nros build --packages-select {name} && nros dev {check} check {name}")`
  at the end of the scaffold, and `readme()`, which writes the same verb
  into the package README.
- `packages/cli/nros-cli-core/src/cmd/new.rs`: the `nros new` help text
  ("Check it with `nros dev runner check <name>` …").
- `packages/cli/nros-cli-core/src/cmd/board_new.rs`, `print_next_steps()`:
  `just check fast`.
- The verbs themselves live in `cmd/extension_check.rs` and
  `cmd/dev.rs` (`BORN_IN_DEV = ["dev runner check", "dev command check"]`).

## What the design says

- RFC-0119 D10 specifies `nros dev runner check <name>` and
  `nros dev command check <name>` and places them in the dev tier. Phase-527
  W7 landed them there.
- RFC-0117 D3's dev-tier table lists `dev runner` / `dev command` as "new:
  `check` for an extension package — RFC-0119".
- RFC-0117 D2's user tier lists `board list|info|new|check` under Inspect,
  and no check for a runner or command package. `nros board check` is also
  not implemented: `cmd/board.rs` `Args` has `List`, `Show`, `Info`, `New`
  and two hidden verbs, and no `Check`.
- No phase plans a user-tier check for runner or command packages. Phases
  527 and 529 (archived) cite only the dev spelling.

So the scaffolds follow the design as written. The gap is in the design: a
user who authors an extension (RFC-0119's stated audience) has no check
verb in their own tier.

## Proposal

- Add a user-tier check, `nros runner check <name>` and
  `nros command check <name>`, with the same implementation and output. The
  `nros dev` spellings stay as silent aliases (RFC-0117 D5). Amend
  RFC-0117 D2 and RFC-0119 D10 to place the check in the user tier and keep
  `dev` as the alias. Another option is one `nros check <package>` that
  dispatches on the package kind. That would also cover a board package and
  replace the `just check fast` line. That line is a second instance of
  the gap, since D2's `nros board check` was never built. Pick one in the
  RFC amendment.
- Update the scaffolds' next steps and README template to the user-tier
  spelling. For an out-of-tree board, print the board check verb once it
  exists instead of `just check fast`. Keep `just check fast` for the
  in-tree arm only, where it is the contributor's acceptance.
- Add a gate or unit test asserting that no `nros new` / `nros board new`
  next-step line names a `nros dev` verb or a `just` recipe on the user
  road.
