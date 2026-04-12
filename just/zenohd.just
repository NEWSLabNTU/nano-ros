set working-directory := '..'

# zenohd module — build the zenohd router from the pinned submodule.

default:
    @just --list --justfile {{source_file()}}

# Build zenohd from the pinned submodule (skipped if already built).
setup:
    #!/usr/bin/env bash
    set -e
    if [ -x build/zenohd/zenohd ]; then
        echo "zenohd already built: build/zenohd/zenohd"
        echo "To rebuild, run: just zenohd clean && just zenohd setup"
    else
        ./scripts/zenohd/build.sh
    fi

# Diagnose zenohd install status (read-only).
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
build:
    ./scripts/zenohd/build.sh

# Clean zenohd build artifacts.
clean:
    ./scripts/zenohd/build.sh --clean
