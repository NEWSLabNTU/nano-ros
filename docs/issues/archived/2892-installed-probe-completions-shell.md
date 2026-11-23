---
id: 2892
title: "The installed-track book probe fails at `nros completions`: the runner's `$SHELL` names no shell it completes in"
status: resolved
type: bug
area: [book, cli, ci]
severity: medium
found: 2026-11-23
related: [phase-590]
---

## What fails

`host-tests.yml` job `book probe — installed track` (run 45104661214, 2026-11-23, main
`90c2d0659f`):

```text
=== probe step 27 (book/src/start/install.md:226) ===
Error: `$SHELL` names no shell `nros` completes in; name one: `nros completions <bash|zsh|fish|powershell>`
  Location: nros-cli-core/src/cmd/completions.rs:33:9
error: recipe `installed` failed on line 40 with exit code 1
```

## Next

Decide which side is wrong. Either the book's step names the shell explicitly, which the error
itself suggests, or `nros completions` with no argument needs a fallback for a probe container
whose `$SHELL` is unset or `sh`. The probe runs the book verbatim, so a reader in the same
situation would hit this too.

## Acceptance

`just probe installed` passes in a scheduled `host-tests` run, and the release rehearsal's
installed-track probe passes on the candidate asset. A release blocker:
[phase-590](../../roadmap/phase-590-nros12-release-plan.md) W1.

## Resolution

Fixed by #4683 (`ff02a051e9`): the book side was wrong. Install step 5 now runs
`nros completions bash`, and the text covers the other shells and what `$SHELL` decides. The
acceptance run is the next scheduled `host-tests` installed-track probe, which phase-590 B7
records.
