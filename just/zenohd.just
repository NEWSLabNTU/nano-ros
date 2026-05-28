set working-directory := '..'

# zenohd module — build the zenohd router from the pinned submodule.

default:
    @just --list --justfile {{source_file()}}

# Provision zenohd (skipped if already present).
#
# Phase 187.6: a thin caller of `nros setup` — fetches the prebuilt zenohd from
# the nano-ros-sdk Releases into build/zenohd/bin/ (or source-builds from the
# index recipe on an unsupported host), then symlinks the flat
# build/zenohd/zenohd path the harness + other recipes read. Version lives only
# in nros-sdk-index.toml. Requires the nros CLI (`just setup base` builds it);
# `just zenohd build` still source-builds from the submodule.
[group("setup")]
setup:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -x build/zenohd/zenohd ]; then
        echo "zenohd present: build/zenohd/zenohd"
        exit 0
    fi
    source scripts/build/cargo.sh
    nros="$(nros_cli_bin)"
    "$nros" setup --tool zenohd --prefix "$(pwd)/build/zenohd" --index "$(pwd)/nros-sdk-index.toml"
    # Bridge the store bin/ layout to the flat path consumers expect.
    ln -sf bin/zenohd build/zenohd/zenohd
    build/zenohd/zenohd --version 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | grep -E '^zenohd v' | tail -1 || true

# Diagnose zenohd install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    if [ -x build/zenohd/zenohd ]; then
        # zenohd logs startup to stderr before printing the version line;
        # strip ANSI escapes and grab the last line that looks like a version.
        ver=$(build/zenohd/zenohd --version 2>&1 \
              | sed 's/\x1b\[[0-9;]*m//g' \
              | grep -E '^zenohd v' \
              | tail -1 || true)
        echo "  [OK] zenohd: build/zenohd/zenohd${ver:+ ($ver)}"
        exit 0
    else
        echo "  [MISSING] zenohd (run: just zenohd setup)"
        exit 1
    fi

# Build zenohd from the pinned submodule (alias for `just zenohd setup`).
[group("main")]
build:
    ./scripts/zenohd/build.sh

# Run the built zenoh router. Blocks the shell until Ctrl-C.
# Accepts any zenohd flags as positional args (forwarded verbatim).
[group("main")]
run *args:
    #!/usr/bin/env bash
    set -e
    if [ ! -x build/zenohd/zenohd ]; then
        echo "ERROR: build/zenohd/zenohd not found. Run: just zenohd setup" >&2
        exit 1
    fi
    exec ./build/zenohd/zenohd {{args}}

# Clean zenohd build artifacts.
[group("main")]
clean:
    ./scripts/zenohd/build.sh --clean
