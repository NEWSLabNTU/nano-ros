set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install PlatformIO Core. Required by `integration_platformio` smoke
# test (Phase 139.6) and by users consuming nano-ros via the
# `integrations/platformio/` shell.
#
# Tries `pipx install platformio` first (isolated, user-local); falls
# back to `pip install --user platformio` when pipx is absent.
# Re-runs are idempotent — `pipx upgrade` / `pip install --upgrade`.
setup:
    #!/usr/bin/env bash
    set -e
    export PATH="$HOME/.local/bin:$PATH"
    if command -v pio >/dev/null 2>&1; then
        echo "PlatformIO already installed: $(pio --version)"
        exit 0
    fi
    if command -v pipx >/dev/null 2>&1; then
        echo "Installing PlatformIO via pipx..."
        pipx install platformio
    else
        echo "pipx not found; falling back to pip install --user platformio"
        echo "(consider: sudo apt install pipx && pipx ensurepath)"
        pip install --user --upgrade platformio
    fi
    if ! command -v pio >/dev/null 2>&1; then
        echo "PlatformIO installed but pio not on PATH."
        echo "Add ~/.local/bin to PATH (or run: pipx ensurepath)."
        exit 1
    fi
    echo "PlatformIO ready: $(pio --version)"

# Diagnose PlatformIO install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    export PATH="$HOME/.local/bin:$PATH"
    if command -v pio >/dev/null 2>&1; then
        echo "  [OK] PlatformIO Core: $(pio --version)"
        exit 0
    else
        echo "  [MISSING] PlatformIO Core (run: just platformio setup)"
        exit 1
    fi
