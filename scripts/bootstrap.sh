#!/usr/bin/env bash
# Bootstrap nano-ros from a fresh checkout without requiring `just` first.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
    cat <<'EOF'
Usage:
  scripts/bootstrap.sh                 # install/check just, then show setup choices
  scripts/bootstrap.sh base            # base quick-start setup
  scripts/bootstrap.sh all             # full contributor / test-all setup
  scripts/bootstrap.sh platform <name> # focused platform setup
  scripts/bootstrap.sh doctor [tier]   # read-only diagnosis

Examples:
  scripts/bootstrap.sh
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

main() {
    case "${1:-}" in
        -h|--help|help)
            usage
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
