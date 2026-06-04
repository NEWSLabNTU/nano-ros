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

# Zephyr workspace path. Default: in-tree (gitignored) `zephyr-workspace/`.
# Falls back to legacy sibling `../nano-ros-workspace` for users set up
# before the in-tree default. The 4.4 line uses its own sibling so the
# two trees coexist. Set $NROS_ZEPHYR_WORKSPACE to override either.
ZEPHYR_WORKSPACE := env("NROS_ZEPHYR_WORKSPACE", if NROS_ZEPHYR_VERSION == "4.4" { "../nano-ros-workspace-4.4" } else if path_exists("zephyr-workspace") == "true" { "zephyr-workspace" } else if path_exists("../nano-ros-workspace") == "true" { "../nano-ros-workspace" } else { "zephyr-workspace" })

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
