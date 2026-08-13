---
id: 568
title: "Tier 1's success banner RAN `just ci-matrix` — backticks in a
  double-quoted recipe echo are shell command substitution"
status: resolved
type: bug
area: build
related: [issue-0466, rfc-0061, phase-318]
---

## Symptom

Every **successful** `just ci` ended with two red lines *after* everything had
passed:

```
error: recipe `_lane-gate` failed with exit code 1
error: recipe `ci-matrix` failed with exit code 1
CI passed (tier 1 — host only; platform coverage needs [tier2] 13 fixture coordinate(s):
  esp32,rust,zenoh
  freertos,mixed,zenoh
  …
```

The banner is not the banner. It is tier 2's *output*, spliced into the middle
of tier 1's sentence — and the `error:` lines above it are a whole `ci-matrix`
invocation that nobody asked for.

## Cause

`justfile:2382`:

```just
    @echo "CI passed (tier 1 — host only; platform coverage needs `just ci-matrix`)!"
```

A recipe line is handed to `sh`. Inside **double** quotes, `` `just ci-matrix` ``
is command substitution, so the last act of a green tier-1 run was to execute
tier 2's lane gate and paste its stdout into the message. The gate fails (tier-1
fixtures do not cover tier 2's coordinates), which is why the noise was errors
rather than a several-hour build — the failure is what kept this cheap, and
therefore what kept it unnoticed.

The prose is right; only the quoting is wrong. `just/*.just` has nine sibling
echoes naming a recipe in backticks and **all nine escape them** (`\`just
setup-cli\``), so this was the one unescaped instance of an idiom the tree
already spells correctly — and `justfile:166` gets it right a second way, with
single quotes.

## Why it hid

The lines are `error: recipe … failed`, printed *below* a run that just said
`All tests passed!`, and *above* a line beginning `CI passed`. Read in order it
looks like a tier-2 note appended to a tier-1 pass. Issue 0466's finding applies
exactly: a tier-1 run has enough legitimate noise that one more red line reads
as scenery. Training a reader to skip `error: recipe … failed` is the real cost
here, not the seconds of gate.

## Fix

Escape the backticks, matching the nine siblings:

```just
    @echo "CI passed (tier 1 — host only; platform coverage needs \`just ci-matrix\`)!"
```

## Sweep

```sh
grep -n 'echo "[^"]*`' justfile just/*.just
```

Ten hits, nine already escaped; this was the tenth. Re-run it after touching any
recipe that names another recipe.
