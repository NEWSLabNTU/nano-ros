#!/usr/bin/env bash
# Bootstrap nano-ros from a fresh checkout without requiring `just` first.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
    cat <<'EOF'
Usage:
  scripts/bootstrap.sh nros            # install the PREBUILT nros CLI (no just/cargo)
  scripts/bootstrap.sh                 # install/check just, then show setup choices
  scripts/bootstrap.sh base            # base quick-start setup
  scripts/bootstrap.sh all             # full contributor / test-all setup
  scripts/bootstrap.sh platform <name> # focused platform setup
  scripts/bootstrap.sh doctor [tier]   # read-only diagnosis

The `nros` path is the just-free user route: it fetches the prebuilt `nros`
binary (Phase 195.A) and you then run `nros setup <board>` / `nros deploy`.
The others are the contributor/source route (rustup + just + `just setup`).

Examples:
  scripts/bootstrap.sh nros
  scripts/bootstrap.sh platform zephyr
  scripts/bootstrap.sh all
EOF
}

ensure_path() {
    case ":$PATH:" in
        *":$HOME/.local/bin:"*) ;;
        *) export PATH="$HOME/.local/bin:$PATH" ;;
    esac
}

install_rustup_if_missing() {
    if command -v cargo >/dev/null 2>&1; then
        return 0
    fi
    if ! command -v curl >/dev/null 2>&1; then
        echo "bootstrap: cargo and curl are missing; install Rust or curl, then rerun." >&2
        exit 1
    fi
    echo "bootstrap: installing rustup so `just` can be installed..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --no-modify-path
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
}

ensure_just() {
    ensure_path
    if command -v just >/dev/null 2>&1; then
        return 0
    fi

    install_rustup_if_missing
    echo "bootstrap: installing just into $HOME/.local/bin..."
    mkdir -p "$HOME/.local"
    cargo install just --locked --root "$HOME/.local"
    ensure_path

    if ! command -v just >/dev/null 2>&1; then
        echo "bootstrap: just installed but not found on PATH; add $HOME/.local/bin." >&2
        exit 1
    fi
}

# Phase 195.A — the just-free prebuilt path: install the `nros` binary from the
# nros-cli Releases (no rustup/just/source build). Prefers the in-tree installer
# (when the codegen submodule is checked out), else fetches it over the network.
install_nros_prebuilt() {
    ensure_path
    if command -v nros >/dev/null 2>&1; then
        echo "bootstrap: nros already on PATH ($(command -v nros))."
    elif [[ -x "${REPO_ROOT}/packages/codegen/install.sh" ]]; then
        echo "bootstrap: installing prebuilt nros (in-tree installer)..."
        "${REPO_ROOT}/packages/codegen/install.sh"
    else
        if ! command -v curl >/dev/null 2>&1; then
            echo "bootstrap: curl required to fetch the nros installer." >&2
            exit 1
        fi
        echo "bootstrap: fetching the nros installer..."
        curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nros-cli/main/install.sh | sh
    fi
    echo "bootstrap: next →  nros setup <board>   then   nros deploy <name>"
}

main() {
    case "${1:-}" in
        -h|--help|help)
            usage
            exit 0
            ;;
        nros)
            install_nros_prebuilt
            exit 0
            ;;
    esac

    ensure_just
    cd "$REPO_ROOT"

    case "${1:-}" in
        "")
            exec just setup
            ;;
        base|quickstart|default|minimal)
            exec just setup base
            ;;
        all|everything|contributor|extended)
            echo "bootstrap: full setup will fetch/install all supported platform SDKs."
            exec just setup all
            ;;
        platform)
            if [[ $# -lt 2 ]]; then
                echo "bootstrap: missing platform name." >&2
                usage >&2
                exit 2
            fi
            exec just "$2" setup
            ;;
        doctor)
            if [[ $# -ge 2 ]]; then
                exec just doctor "$2"
            fi
            exec just doctor
            ;;
        *)
            echo "bootstrap: unknown command: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
}

main "$@"
