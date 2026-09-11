---
id: 1318
title: "`from_env_honours_the_domain_override` fails about one run in three inside its own binary and passes alone — a process-env race in `nros`'s init tests"
status: open
type: bug
area: testing, api
severity: low
found: 2026-09-11
related: [phase-448, issue-1183]
---

## Measured

`just check node-std-tests` runs `cargo test -p nros --lib --features env,std`.
On 2026-09-11, on `main` + phase-448 W7/W8 (a branch that touches neither
`packages/api/nros` nor anything the test reads), three consecutive runs of that
exact command on one unchanged tree:

| run | result |
| --- | --- |
| 1 | 85 passed |
| 2 | 84 passed, **1 failed** |
| 3 | 85 passed |

The failure, every time it appears:

    thread 'init::ros_args_refusal_tests::from_env_honours_the_domain_override'
    panicked at packages/api/nros/src/init.rs:737:23:
    init() failed on the test host: EnvParseFailed

Run SOLO (`-- --exact
init::ros_args_refusal_tests::from_env_honours_the_domain_override`) it passes,
repeatedly.

## Why this is a race and not a host fact

`EnvParseFailed` is raised from a read of the PROCESS environment, and nextest
runs the tests in one binary as threads of one process. A sibling test that sets
or clears a `ROS_*` / `NROS_*` variable between this test's write and its read
produces exactly this: intermittent, binary-scoped, invisible alone. It is the
third process-global in this tree found the same way — `executor::backing::TAKEN`
(issues 1183/1186) and the boot record (issue 1036) are the other two, and both
were answered by giving the test its own process.

Nothing was measured about WHICH sibling; the reproduction above is all this
issue claims.

## Why it matters more than a flake usually does

`node-std-tests` is a gate. A gate that is red one run in three is a gate whose
verdict nobody can read: the next real regression in that lane looks exactly
like this. It cost one re-run here and was only recognised as a flake because
the same command was run three times on purpose.

## Candidate fixes, in the order this tree has used before

1. Give it its own process, the way `node-std-tests` already does for the boot
   record and the backing latch (`ran_tests "..." cargo test ... -- --exact`).
   Cheapest, and it is the established shape.
2. Find the sibling that mutates the shared variable and make the mutation
   scoped. Better, but it needs the measurement this issue did not take.

Do not answer it with a retry: a retry turns "one run in three is red" into
"one run in twenty-seven is red", which is the same defect with a longer fuse.
