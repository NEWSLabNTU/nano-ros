set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}
# Zephyr line selector (Phase 180.A). "3.7" = LTS (default, west.yml);
# "4.4" = latest rolling (west-4.4.yml, sibling workspace). Set
# $NROS_ZEPHYR_VERSION to pick. Support policy + add-a-line checklist:
# docs/development/zephyr-version-support.md (floor is 3.7 — the
# zephyr-lang-rust × zephyr pair; nothing below ships the Rust module).
NROS_ZEPHYR_VERSION := env("NROS_ZEPHYR_VERSION", "3.7")

# West manifest for the selected line.
ZEPHYR_MANIFEST := if NROS_ZEPHYR_VERSION == "4.4" { "west-4.4.yml" } else { "west.yml" }

# Zephyr workspace path — ONE resolver, phase-440 W1 / RFC-0095 D4.
#
# The ladder used to be spelled here as a `just` expression, again as a `for`
# loop in `scripts/build/west-fixtures.sh`, and a third time in
# `scripts/check-tier-preconditions.sh` — and the three DISAGREED (measured: on
# the 4.4 line this arm named the 4.4 sibling while west-fixtures resolved the
# 3.7 in-tree workspace). A `just` expression and a shell function cannot share
# code, so `just` CALLS the helper rather than restating it. What it walks:
# `$NROS_ZEPHYR_WORKSPACE` (wins outright — it is the INSTALL target too, so it
# must resolve before the tree exists), the checkout-relative trees for THIS
# line, then `$NROS_STORE/workspaces/zephyr/<version>`. The store arm is LAST
# so W1 moves nothing; W4 inverts it. `check-zephyr-workspace-resolvers` refuses
# a fourth spelling.
#
# `resolve-or-default`, not `resolve`: `just zephyr setup` needs a path whether
# or not one is provisioned, and the default is where it will install.
ZEPHYR_WORKSPACE := shell("scripts/lib/zephyr-workspace.sh --version $1 resolve-or-default", NROS_ZEPHYR_VERSION)

# Python 3.12 venv bin for the 4.4 line (Task 2b). 4.4's
# find_package(Python3) requires >=3.12; the venv provisions it without
# sudo. Recipes that invoke `west` on the 4.4 line prepend this to PATH
# so bare `west` resolves to the venv's (3.12) one. Empty on 3.7.
ZEPHYR_VENV_BIN := if NROS_ZEPHYR_VERSION == "4.4" { ZEPHYR_WORKSPACE / ".venv312/bin" } else { "" }

# Install the Zephyr workspace at $repo/zephyr-workspace/ (large download).
# Idempotent: skips if the workspace already exists.
# Set $NROS_ZEPHYR_WORKSPACE to install elsewhere.
# Extra args are forwarded: e.g. `just zephyr setup --force --skip-sdk`.

import "zephyr-setup.just"
import "zephyr-ci.just"
import "zephyr-dev.just"
