set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install PlatformIO Core. Required by the `cli_bringup_platformio`
# test and by users consuming nano-ros via the
# `integrations/platformio/` extra_script adapter.
#
# `pipx install platformio` (isolated, user-local) — and nothing else. An
# existing `pio` on PATH satisfies the recipe; without pipx it reports and
# stops rather than falling back to `pip install --user`, which PEP 668 hosts
# refuse anyway. Re-runs are idempotent.
[group("setup")]
setup:
    #!/usr/bin/env bash
    set -e
    export PATH="$HOME/.local/bin:$PATH"
    if command -v pio >/dev/null 2>&1; then
        echo "PlatformIO already installed: $(pio --version)"
        exit 0
    fi
    # pipx only. The `pip install --user platformio` fallback is gone (issue
    # 0698 follow-up): PEP 668 hosts (Arch, Fedora, Debian 12+) refuse `--user`
    # outright, so the "fallback" failed exactly where pipx was most likely to
    # be absent, and where it DID work it put a tool into a system interpreter
    # nano-ros does not own.
    #
    # pipx is the right shape for this one because PlatformIO is an
    # APPLICATION, not a library our build imports: pipx gives it a private
    # interpreter and puts `pio` on PATH, so nothing else on the host moves.
    # When pipx is absent we say so and stop, rather than reaching for the
    # mechanism that is refused on the same hosts.
    if ! command -v pipx >/dev/null 2>&1; then
        echo "pipx not found — nano-ros does not install PlatformIO any other way." >&2
        echo "" >&2
        echo "  PlatformIO is an application, so it wants its own interpreter:" >&2
        echo "      <distro pkg manager> install pipx     # e.g. pacman -S python-pipx" >&2
        echo "      pipx ensurepath" >&2
        echo "" >&2
        echo "  Then: just platformio setup" >&2
        echo "  (or install PlatformIO yourself and put \`pio\` on PATH — this" >&2
        echo "   recipe is satisfied by any \`pio\` it can find.)" >&2
        exit 1
    fi
    echo "Installing PlatformIO via pipx..."
    pipx install platformio
    if ! command -v pio >/dev/null 2>&1; then
        echo "PlatformIO installed but pio not on PATH."
        echo "Add ~/.local/bin to PATH (or run: pipx ensurepath)."
        exit 1
    fi
    echo "PlatformIO ready: $(pio --version)"

# Diagnose PlatformIO install status (read-only).
[group("setup")]
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

# Clean PlatformIO integration example build artifacts.
[group("main")]
clean:
    #!/usr/bin/env bash
    if [ -d integrations/platformio/examples ]; then
        find integrations/platformio/examples -type d \( -name .pio -o -name build \) -exec rm -rf {} + 2>/dev/null || true
    fi
    echo "PlatformIO build artifacts cleaned"
