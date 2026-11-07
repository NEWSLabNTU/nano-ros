---
id: 2294
title: "The runner, command and board scaffolds name `nros dev … check` and `just check fast` as the next step, so a user has no check step for a package they made"
status: resolved
type: enhancement
area: cli, ux, scaffold
related: [rfc-0117, rfc-0119, phase-527, phase-537, 2291]
found: 2026-11-06
resolved: 2026-11-07
resolved_in: "fix-issue-2294 branch: one user-tier `nros check <package>`, dispatching on the package kind"
---

**Home: RFC-0117 Amendment 2026-11-07** (user ruling). Reported by the
phase-537 W12 book session with an in-tree CLI.

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

## Resolution

User ruling, 2026-11-07: one user-tier verb, `nros check <package>`, which
works out the package's kind and runs that kind's check. Recorded as RFC-0117
Amendment 2026-11-07 (D2, D3) and in RFC-0119 D10.

- **Telling a package from the workspace forms.** `nros check` already took a
  path positional (default `build/nros/nros-plan.json`) and `--workspace`. A
  positional that exists on disk, or holds `/` or `.`, is a path as before. A
  bare name (`[A-Za-z0-9_-]`) with nothing of that name on disk is a package
  (`check::package_name_of`). No path form could read such a name, so every
  command that worked before means the same thing. `--workspace` and
  `--bringup` are unchanged.
- **Finding the kind.** `extension_check::kinds_of` reads the provider scan
  and the board catalog a build uses. A name matches a provision by its own
  name or by its package's name (`acme` and `nros_board_acme` both find the
  scaffolded board), and also matches a board by any catalog alias. A runner
  gets the existing runner check and a command the command check. A package
  of both kinds gets both checks.
- **The board check** (`board::board_check`) reuses the validation that
  already existed. The descriptor resolves through the catalog, which parses
  it and requires its `package.xml` announcement. The announced names must
  equal the declared ones (`check-provider-announcements` A2, applied to one
  package outside the tree). `[board.net]` must resolve, and a
  `[board.zephyr]` board must project into the cmake variables `nros board
  info` prints. No `nros board check` was added; this is that verb.
- **Scaffolds.** `nros new <name> --kind runner|command` prints `nros build
  --packages-select <name> && nros check <name>`, and its README says
  `nros check <name>`. `nros board new` prints `nros check <key>` on both
  arms, and `just check fast` stays on the in-tree arm only, after it. The
  `nros new --kind` help names the check, and the `nros` quick start says
  `nros check` checks a package. `nros dev runner|command check`
  stay as silent aliases. `scripts/check-template-copy-out.sh`'s runner
  acceptance now runs `nros check acme_runner --no-build`.

Tests: `check::tests::a_bare_name_not_on_disk_is_a_package_and_every_path_stays_a_path`,
`the_workspace_form_and_the_package_form_parse`,
`extension_check::tests::a_name_finds_the_kind_of_its_package`,
`a_package_of_two_kinds_gets_both_checks`,
`board_new::tests::the_next_steps_name_the_user_check`,
`a_scaffolded_board_passes_the_board_check` (each platform, plus a drifted
announcement that must fail),
`new_extension::tests::the_next_step_and_the_readme_name_the_user_check`, and
in `nros-cli/tests/board_new_scaffold.rs`
`nros_check_runs_the_board_check_on_a_scaffolded_board` (end to end, from a
directory in no checkout) and the extended
`the_out_of_tree_next_steps_name_no_contributor_step`.
