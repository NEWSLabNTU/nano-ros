---
id: 950
title: "`macro_deploy_token` hands an mps2 image `DEPLOY freertos`, and `--deploy` outranks `--board`, so it resolves a boardless target"
status: open
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
