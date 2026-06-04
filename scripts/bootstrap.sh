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

# Phase 218 — the just-free `nros` path: previously fetched a prebuilt
# binary from `nros-cli` Releases. After the Phase 218 monorepo merge,
# `nros` ships from the in-tree sub-workspace at `packages/cli/`. Three
# acquisition paths, in priority order:
#   1. Already on PATH (a previous `just setup-cli` or activate.sh) — no-op.
#   2. Tagged checkout — `scripts/install-nros-prebuilt.sh` fetches the
#      matching `nros-<triple>.tar.gz` from the GitHub release and
#      installs to `packages/cli/target/release/nros`.
#   3. Branch / development checkout — build from source via
#      `cargo build --release --manifest-path packages/cli/Cargo.toml
#      --bin nros`. Needs rustup; install via this script's `base`
#      subcommand first if not present.
install_nros_prebuilt() {
    ensure_path
    if command -v nros >/dev/null 2>&1; then
        echo "bootstrap: nros already on PATH ($(command -v nros))."
        return 0
    fi

    # Path 2: tagged checkout → prebuilt fetch.
    if [[ -x "${REPO_ROOT}/scripts/install-nros-prebuilt.sh" ]]; then
        if git -C "${REPO_ROOT}" describe --tags --abbrev=0 --match 'nros-v*' >/dev/null 2>&1; then
            echo "bootstrap: tagged checkout — fetching prebuilt nros..."
            "${REPO_ROOT}/scripts/install-nros-prebuilt.sh"
            export PATH="${REPO_ROOT}/packages/cli/target/release:$PATH"
            echo "bootstrap: next →  nros setup <board>   then   nros deploy <name>"
            return 0
        fi
    fi

    # Path 3: build from source. The CLI sub-workspace builds in ~30s on
    # a fresh checkout. Requires cargo on PATH.
    if ! command -v cargo >/dev/null 2>&1; then
        echo "bootstrap: cargo not on PATH; run 'scripts/bootstrap.sh base' first." >&2
        exit 1
    fi
    echo "bootstrap: building nros from packages/cli/ (Phase 218)..."
    (cd "${REPO_ROOT}" && cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros)
    export PATH="${REPO_ROOT}/packages/cli/target/release:$PATH"
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
