set working-directory := '..'

# Path to the ESP-IDF workspace created by `scripts/esp_idf/setup.sh`.
# Default mirrors the setup script's NROS_ESP_IDF_WORKSPACE default.
ESP_IDF_WORKSPACE := env_var_or_default('NROS_ESP_IDF_WORKSPACE', justfile_directory() + '/../esp-idf-workspace/esp-idf')

default:
    @just --list --justfile {{source_file()}}

# Install ESP-IDF (clone + ./install.sh). Opt-in — NOT pulled by top-level
# `just setup`, which uses esp-hal bare-metal instead. Required only when
# driving the nros-platform-esp-idf-c C port.
#
# Re-running is idempotent: refreshes the checkout + re-runs install.sh.
setup *args:
    @bash scripts/esp_idf/setup.sh {{args}}

# Diagnose ESP-IDF install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -f "{{ESP_IDF_WORKSPACE}}/export.sh" ]; then
        echo "  [OK] esp-idf checkout: {{ESP_IDF_WORKSPACE}}"
    else
        echo "  [MISSING] esp-idf checkout at {{ESP_IDF_WORKSPACE}} (run: just esp_idf setup)"
        fail=1
    fi
    if [ -f "esp-idf-workspace/env.sh" ]; then
        echo "  [OK] env.sh shim"
    else
        echo "  [MISSING] esp-idf-workspace/env.sh (run: just esp_idf setup)"
        fail=1
    fi
    if [ "$fail" -eq 0 ]; then
        # Probe a target chip the install actually pulled.
        if (source esp-idf-workspace/env.sh >/dev/null 2>&1 && command -v idf.py >/dev/null 2>&1); then
            echo "  [OK] idf.py reachable after sourcing env.sh"
        else
            echo "  [WARN] env.sh does not expose idf.py; re-run setup."
            fail=1
        fi
    fi
    exit $fail

# Build the ESP-IDF smoke project that pulls nros-platform-esp-idf-c.
# Targets are taken from IDF_TARGET (default: esp32c3).
build-c-port target='esp32c3':
    #!/usr/bin/env bash
    set -e
    if [ ! -f esp-idf-workspace/env.sh ]; then
        echo "esp-idf-workspace/env.sh missing — run: just esp_idf setup" >&2
        exit 2
    fi
    source esp-idf-workspace/env.sh
    cd tests/esp-idf-c-smoke
    idf.py set-target "{{target}}"
    idf.py build

# Run the smoke project on qemu-system-{riscv32,xtensa}. Requires the
# Espressif QEMU fork (installed via `just esp32 setup`).
test-c-port target='esp32c3':
    #!/usr/bin/env bash
    set -e
    just esp_idf build-c-port "{{target}}"
    source esp-idf-workspace/env.sh
    cd tests/esp-idf-c-smoke
    case "{{target}}" in
        esp32c3) qemu=qemu-system-riscv32 ;;
        esp32)   qemu=qemu-system-xtensa ;;
        *)       echo "qemu target not wired for {{target}}" >&2; exit 1 ;;
    esac
    if ! command -v "$qemu" >/dev/null; then
        echo "$qemu not installed — run: just esp32 setup" >&2
        exit 2
    fi
    # idf.py builds a merged binary at build/<project>.bin; pass to qemu.
    project_bin=$(ls build/*.bin 2>/dev/null | head -1)
    if [ -z "$project_bin" ]; then
        echo "no build artifact found" >&2
        exit 1
    fi
    timeout 30 "$qemu" -nographic -machine {{target}} \
        -drive "file=$project_bin,if=mtd,format=raw" -nic none

# Clean smoke-project build.
clean:
    rm -rf tests/esp-idf-c-smoke/build tests/esp-idf-c-smoke/sdkconfig*

# CI: doctor + build-c-port.
ci: doctor build-c-port
