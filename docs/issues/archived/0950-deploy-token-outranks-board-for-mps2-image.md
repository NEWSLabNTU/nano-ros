---
id: 950
title: "`macro_deploy_token` hands an mps2 image `DEPLOY freertos`, and `--deploy` outranks `--board`, so it resolves a boardless target"
status: resolved
area: build
severity: medium
found: 2026-08-31
related: [0941, 0949, phase-405, RFC-0072]
---

# The more specific answer loses to the less specific question

`macro_deploy_token` gives an mps2-an385-freertos image the deploy token
`freertos`. In `nros ws board-facts`, `--deploy` outranks `--board`. So the
resolver is asked about `[deploy.freertos]` — which names no `board` — while
`--board mps2-an385-freertos` alone would have resolved all five values from
`[deploy.mps2-an385-freertos]`.

Post-0940 the boardless `[deploy.freertos]` no longer carries an unreachable
`.nros` block, so this is not the 0940 defect. It is the adjacent one: the image
HAS a correct answer available and the precedence order picks the target that
cannot give it.

Measured on the 0940-fixed tree:

```
$ nros ws board-facts --deploy freertos examples/workspaces/c/src/demo_bringup
Error: board-facts[deploy-names-no-board]: [deploy.freertos] names no `board`

$ nros ws board-facts --board mps2-an385-freertos examples/workspaces/c/src/demo_bringup
NROS_BOARD=mps2-an385-freertos
NROS_BOARD_TOML=.../nros-board-mps2-an385-freertos/nros-board.toml
NROS_NETSTACK=lwip
NROS_SDK_FREERTOS=...
NROS_SDK_LWIP=...
```

## The judgement

Three candidate fixes, and choosing needs a look at what the deploy token is
FOR:

1. `macro_deploy_token` should emit the board-named deploy for a board-specific
   image;
2. or `--deploy` should fall back to `--board` when the named deploy names no
   board and a `--board` was also supplied;
3. or the boardless `[deploy.freertos]` alias should gain a `board`.

(2) is tempting and is the one to be most careful with — a silent fallback
between two selectors is how a wrong answer becomes an invisible one, which is
the whole subject of 0941.

Found while implementing 0941.

## Resolved — SUPERSEDED by #0951 (2026-08-31)

The dangerous half is gone, structurally.

This issue's sharpest finding was that 54 of 335 both-selector probes returned a
DIFFERENT BOARD's facts: `--board mps2-an385-freertos --deploy threadx-linux`
answered `NROS_BOARD=threadx-linux`, silently, because `pick_deploys` used an
`else if` so `--deploy` replaced the question rather than breaking a tie.

Under #0951 `--deploy` resolves against `[image.*]`, and the same probe now
**errors**:

```
$ nros ws board-facts --board mps2-an385-freertos --deploy threadx-linux <bringup>
Error: .../system.toml: no [image.threadx-linux]
```

while the agreeing case still answers correctly:

```
$ nros ws board-facts --board mps2-an385-freertos --deploy freertos <bringup>
NROS_BOARD=mps2-an385-freertos
```

So a wrong answer became an error rather than a quieter wrong answer. The `else
if` shape remains in the source, but its consequence does not: a deploy token
that names no image cannot resolve to some other board's facts.

The phase-405 fix (a `board-facts-note[...]` announced fallback with the board
authoritative) was dropped rather than rebased. It solved the same problem by
ANNOUNCING the mismatch; #0951 solved it by making the mismatch unrepresentable,
which is the better shape and the one this repo prefers (issue 0380).
