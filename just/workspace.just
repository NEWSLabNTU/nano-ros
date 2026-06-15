set working-directory := '..'

NIGHTLY := `awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml`

# Pinned ninja version. Ubuntu's apt `ninja-build` lags (1.10); we need
# ≥1.13 for the GNU-jobserver client (Phase 176). Installed as a static
# binary under `third-party/ninja/`; `.envrc` prepends it to PATH.
NINJA_VERSION := "1.13.2"

# Pinned GNU make version. Ubuntu ships 4.3, whose jobserver is
# pipe-fd-based; ninja 1.13's jobserver client speaks ONLY the
# fifo protocol (make ≥4.4, `--jobserver-style=fifo`). Phase 176's
# unified jobserver needs 4.4. Built from source to third-party/make/;
# `.envrc` prepends it to PATH.
MAKE_VERSION := "4.4.1"

# Pinned Corrosion version. Corrosion is the cmake-side Rust bridge used
# by mixed-language workspaces (Phase 212 — `corrosion_import_crate` in
# `cmake/NanoRosThreadxSystemCodegen.cmake` etc.). Installed to
# `~/.nros/sdk/corrosion/` so it can be discovered by `find_package(
# Corrosion)` without polluting the host system; the H.4 and 212.D
# Corrosion tests look it up via `cmake --find-package` with
# `CMAKE_PREFIX_PATH=$HOME/.nros/sdk/corrosion`. Phase 212.H.4.
CORROSION_VERSION := "v0.5.1"

# Pinned play_launch_parser SHA. Used by the Phase 212.L.6 launch-graph
# resolver in `nros plan` / `nros codegen-system` (synth-from-Component
# + multi-launch precedence). No upstream release tags exist on
# `jerry73204/play_launch_parser` — pinned to a SHA on `main` (same
# convention `[tool.cyclonedds]` uses); bumping is the signal to re-run
# the L.6 gate tests `phase212_l6_launch_synth::*`. Kept in lockstep
# with `nros-sdk-index.toml::[tool.play_launch_parser]` so a
# `just workspace install-play-launch-parser` and a
# `nros setup --tool play_launch_parser` land the same revision.
PLAY_LAUNCH_PARSER_VERSION := "098ccb43328bca3c35320bb3adac27a11035b15d"

# Workspace module — host-agnostic dev environment setup.
#
# "workspace" here means the developer's dev workspace (host OS packages,
# Rust toolchains, cargo tools) — NOT the Cargo `[workspace]` term-of-art.
# These recipes configure the host machine to be able to do anything with
# this repo. Platform-specific setup lives in the per-platform modules
# (just/esp32.just, just/nuttx.just, etc.).

default:
    @just --list --justfile {{source_file()}}

# Install all workspace prerequisites (apt + rust + cargo tools).
#
# Rust toolchain channels and components are driven by the
# `rust-toolchain.toml` files checked into the repo (root, tools/,
# examples/**/, packages/verification/nros-verification/). No bare
# `stable`/`nightly` channels are installed here — every version is
# pinned in a TOML file and installed by `rust-pinned-toolchains`.
[group("setup")]
setup:
    @just workspace apt-packages
    @just workspace install-ninja
    @just workspace install-make
    @just workspace install-corrosion
    @just workspace install-play-launch-parser
    @just workspace rust-pinned-toolchains
    @just workspace rust-targets
    @just workspace cargo-tools
    @just workspace build-codegen
    @just workspace install-sccache

# Verify the host `nros` build tool resolves (the message/IDL codegen used by
# `nros generate-*`, the cmake `nano_ros_generate_interfaces()` path, and the
# NuttX external-app FFI staging). Phase 218 — `nros` is built in-tree at
# `packages/cli/target/release/nros` (the `cargo-tools` step above runs
# `just setup-cli`); this is the setup-stage presence check. Build recipes
# keep `nros_cargo_ensure_codegen_c` as a net.
[group("setup")]
build-codegen:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/build/cargo.sh
    nros_cargo_ensure_codegen_c
    echo "[codegen] nros tool: $(nros_cargo_codegen_c_bin)"

# Build the pinned GNU make (>=4.4) from source under third-party/make/.
# Ubuntu's make 4.3 jobserver is pipe-fd-based; ninja 1.13's jobserver
# client only speaks fifo (make 4.4 `--jobserver-style=fifo`). Needed
# for the Phase 176 unified jobserver. `.envrc` puts it first on PATH.
# Lives under third-party/ (survives `just clean`). Idempotent.
[group("setup")]
install-make:
    #!/usr/bin/env bash
    set -euo pipefail
    want="{{MAKE_VERSION}}"
    dest="third-party/make"
    bin="$dest/make"
    if [ -x "$bin" ] && "$bin" --version 2>/dev/null | head -1 | grep -q "$want"; then
        echo "[make] $want already installed ($bin)"
        exit 0
    fi
    mkdir -p "$dest"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    echo "[make] downloading $want..."
    curl -fSL "https://ftp.gnu.org/gnu/make/make-$want.tar.gz" -o "$tmp/make.tar.gz"
    tar xzf "$tmp/make.tar.gz" -C "$tmp"
    echo "[make] building (./configure && ./build.sh)..."
    ( cd "$tmp/make-$want" && ./configure >/dev/null 2>&1 && ./build.sh >/dev/null 2>&1 )
    cp "$tmp/make-$want/make" "$bin"
    chmod +x "$bin"
    # Some sub-builds (cmake generators, RTOS kernel makefiles) invoke
    # `gmake`, not `make`. Provide a gmake -> make 4.4 alias so they too
    # are fifo-jobserver-capable; otherwise a stray make 4.3 chokes on
    # the inherited `--jobserver-auth=fifo:` in MAKEFLAGS.
    ln -sf make "$dest/gmake"
    echo "[make] installed: $("$bin" --version | head -1) ($bin) (+ gmake alias)"
    echo "[make] ensure third-party/make is on PATH (direnv .envrc does this)."

# Install the pinned ninja static binary under third-party/ninja/.
# Ubuntu's apt ninja is too old (1.10) for the GNU-jobserver client;
# `.envrc` puts third-party/ninja first on PATH so this one wins. Lives
# under third-party/ (not build/) so `just clean` doesn't wipe it.
# Idempotent: skips when the pinned version is already present.
[group("setup")]
install-ninja:
    #!/usr/bin/env bash
    set -euo pipefail
    want="{{NINJA_VERSION}}"
    dest="third-party/ninja"
    bin="$dest/ninja"
    if [ -x "$bin" ] && [ "$("$bin" --version 2>/dev/null)" = "$want" ]; then
        echo "[ninja] $want already installed ($bin)"
        exit 0
    fi
    case "$(uname -m)" in
        x86_64)  asset="ninja-linux.zip" ;;
        aarch64) asset="ninja-linux-aarch64.zip" ;;
        *) echo "[ninja] unsupported arch $(uname -m); install ninja >=1.13 manually" >&2; exit 1 ;;
    esac
    url="https://github.com/ninja-build/ninja/releases/download/v$want/$asset"
    echo "[ninja] downloading $want ($asset)..."
    mkdir -p "$dest"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    curl -fSL "$url" -o "$tmp/ninja.zip"
    unzip -o "$tmp/ninja.zip" -d "$dest" >/dev/null
    chmod +x "$bin"
    echo "[ninja] installed: $("$bin" --version) ($bin)"
    echo "[ninja] ensure third-party/ninja is on PATH (direnv .envrc does this)."

# Install Corrosion (cmake ↔ Rust bridge) into `~/.nros/sdk/corrosion/`.
# Mixed-language workspaces (Phase 212) use `find_package(Corrosion)` +
# `corrosion_import_crate()` to pull Rust component staticlibs into a
# cmake build. Lives under `~/.nros/sdk/` (not third-party/) because it's
# a per-developer install prefix consumed via `CMAKE_PREFIX_PATH`, not a
# repo-relative tool. Idempotent: skips when the pinned tag is already
# installed.
[group("setup")]
install-corrosion:
    #!/usr/bin/env bash
    set -euo pipefail
    want="{{CORROSION_VERSION}}"
    prefix="$HOME/.nros/sdk/corrosion"
    stamp="$prefix/.installed-version"
    if [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$want" ]; then
        echo "[corrosion] $want already installed ($prefix)"
        exit 0
    fi
    mkdir -p "$prefix"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    echo "[corrosion] cloning $want..."
    git clone --depth=1 --branch "$want" https://github.com/corrosion-rs/corrosion.git "$tmp/src"
    echo "[corrosion] configuring..."
    cmake -S "$tmp/src" -B "$tmp/build" \
        -DCMAKE_INSTALL_PREFIX="$prefix" \
        -DCORROSION_BUILD_TESTS=OFF >/dev/null
    echo "[corrosion] building..."
    cmake --build "$tmp/build" >/dev/null
    echo "[corrosion] installing to $prefix..."
    cmake --install "$tmp/build" >/dev/null
    echo "$want" > "$stamp"
    echo "[corrosion] installed $want at $prefix"
    echo "[corrosion] consumers should pass CMAKE_PREFIX_PATH=$prefix or set it globally."

# Install play_launch_parser into `~/.nros/sdk/play_launch_parser/`.
# The Phase 212.L.6 launch-graph resolver in `nros plan` /
# `nros codegen-system` shells out to this binary for both synth (a
# Component pkg without a launch file) and multi-launch resolution
# (`<pkg>.launch.xml` > `system.launch.xml` precedence). Lives under
# `~/.nros/sdk/` (not third-party/) because it's a per-developer
# install consumed via PATH, not a repo-relative tool. Idempotent:
# skips when the pinned SHA is already installed. Pinned to a SHA on
# `main` (no upstream tags). Matches `[tool.play_launch_parser]` in
# `nros-sdk-index.toml` so the just-recipe and `nros setup --tool` land
# the same revision. `.envrc` adds `~/.nros/sdk/play_launch_parser/bin`
# to PATH so the L.6 gate tests' `play_launch_parser_available()` probe
# (`Command::new("play_launch_parser").arg("--help")`) finds it.
[group("setup")]
install-play-launch-parser:
    #!/usr/bin/env bash
    set -euo pipefail
    want="{{PLAY_LAUNCH_PARSER_VERSION}}"
    prefix="$HOME/.nros/sdk/play_launch_parser"
    stamp="$prefix/.installed-version"
    if [ -f "$stamp" ] && [ "$(cat "$stamp")" = "$want" ]; then
        echo "[play_launch_parser] $want already installed ($prefix)"
        exit 0
    fi
    mkdir -p "$prefix"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    echo "[play_launch_parser] cloning $want..."
    git clone https://github.com/jerry73204/play_launch_parser.git "$tmp/src"
    git -C "$tmp/src" checkout --quiet "$want"
    echo "[play_launch_parser] building (cargo install)..."
    cargo install --path "$tmp/src/crates/play_launch_parser" \
        --root "$prefix" --locked >/dev/null
    echo "$want" > "$stamp"
    echo "[play_launch_parser] installed $want at $prefix"
    echo "[play_launch_parser] ensure $prefix/bin is on PATH (direnv .envrc does this)."

# Install sccache (shared compiler cache) into the SDK store via the index
# (issue #74). The justfile's `RUSTC_WRAPPER` + the zephyr fixture CMake launcher
# auto-use it once on PATH (activate.{sh,fish} adds the store bin) — ~46% faster
# host / ~22% embedded clean rebuilds (docs/development/build-ux-audit.md).
# Non-fatal: sccache only speeds builds, so a failed source build warns rather
# than aborting setup. Skipped when sccache is already on PATH.
[group("setup")]
install-sccache:
    #!/usr/bin/env bash
    set -uo pipefail
    if command -v sccache >/dev/null 2>&1; then
        echo "[sccache] already on PATH ($(command -v sccache)) — skipping"
        exit 0
    fi
    source scripts/build/cargo.sh
    nros_bin="$(nros_cli_bin 2>/dev/null || true)"
    if [ -z "${nros_bin:-}" ]; then
        echo "[sccache] nros CLI not built yet — run 'just setup-cli', then 'just workspace install-sccache'"
        exit 0
    fi
    echo "[sccache] provisioning via 'nros setup --tool sccache' (source build, one-time)..."
    "$nros_bin" setup --tool sccache \
        || echo "[sccache] provisioning failed (optional cache) — install manually for faster builds"

# Diagnose workspace install status (read-only).
# Returns exit 1 if anything is missing.
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    check() {
        if "$@" &>/dev/null; then return 0; else return 1; fi
    }

    # --- apt-packages ---
    apt_missing=()
    check command -v arm-none-eabi-gcc  || apt_missing+=(gcc-arm-none-eabi)
    check command -v qemu-system-arm    || apt_missing+=(qemu-system-arm)
    check command -v qemu-system-riscv64 || apt_missing+=(qemu-system-misc)
    check command -v cmake               || apt_missing+=(cmake)
    check command -v socat               || apt_missing+=(socat)
    check command -v riscv64-unknown-elf-gcc || apt_missing+=(gcc-riscv64-unknown-elf)
    [ -f /usr/include/mbedtls/ssl.h ] || apt_missing+=(libmbedtls-dev)
    if [ ${#apt_missing[@]} -eq 0 ]; then
        echo "  [OK] apt-packages"
    else
        echo "  [MISSING] apt-packages: ${apt_missing[*]} (run: just workspace apt-packages)"
        fail=1
    fi

    # --- pinned ninja (>=1.13 for the GNU-jobserver client, Phase 176) ---
    ninja_bin="third-party/ninja/ninja"
    if [ -x "$ninja_bin" ] && [ "$("$ninja_bin" --version 2>/dev/null)" = "{{NINJA_VERSION}}" ]; then
        echo "  [OK] ninja {{NINJA_VERSION}} (third-party/ninja; jobserver-capable)"
    else
        echo "  [MISSING] ninja {{NINJA_VERSION}} (run: just workspace install-ninja)"
        fail=1
    fi

    # --- Corrosion (cmake ↔ Rust bridge, Phase 212) ---
    corrosion_stamp="$HOME/.nros/sdk/corrosion/.installed-version"
    if [ -f "$corrosion_stamp" ] && [ "$(cat "$corrosion_stamp" 2>/dev/null)" = "{{CORROSION_VERSION}}" ]; then
        echo "  [OK] corrosion {{CORROSION_VERSION}} (~/.nros/sdk/corrosion)"
    else
        echo "  [MISSING] corrosion {{CORROSION_VERSION}} (run: just workspace install-corrosion)"
        fail=1
    fi

    # --- play_launch_parser (Phase 212.L.6 launch-graph resolver) ---
    # Phase 220.J.2 — FAIL (not warn) on either missing install OR
    # binary-not-on-PATH. `nros plan` shells out to `play_launch_parser`
    # via PATH lookup; install-stamp alone is insufficient — agents
    # routinely hit the gap when a fresh worktree forgets `source
    # ./activate.sh` (the script that PATH-prepends
    # `~/.nros/sdk/play_launch_parser/bin/`). Tailored hints distinguish
    # the two failure modes.
    plp_stamp="$HOME/.nros/sdk/play_launch_parser/.installed-version"
    plp_bin="${NROS_HOME:-$HOME/.nros}/sdk/play_launch_parser/bin/play_launch_parser"
    plp_install_ok=0
    if [ -f "$plp_stamp" ] && [ "$(cat "$plp_stamp" 2>/dev/null)" = "{{PLAY_LAUNCH_PARSER_VERSION}}" ] && [ -x "$plp_bin" ]; then
        plp_install_ok=1
    fi
    if [ $plp_install_ok -eq 1 ] && command -v play_launch_parser >/dev/null 2>&1; then
        echo "  [OK] play_launch_parser {{PLAY_LAUNCH_PARSER_VERSION}} (~/.nros/sdk/play_launch_parser)"
    elif [ $plp_install_ok -eq 1 ]; then
        echo "  [PATH] play_launch_parser installed at ~/.nros/sdk/play_launch_parser/bin/ but not on PATH —"
        echo "         run: source ./activate.sh  (or direnv allow / source ./activate.fish)"
        echo "         (Phase 220.J — agent driver scripts MUST source activate.sh before any just <plat> build)"
        fail=1
    else
        echo "  [MISSING] play_launch_parser {{PLAY_LAUNCH_PARSER_VERSION}} (run: just workspace install-play-launch-parser)"
        fail=1
    fi

    # --- pinned make (>=4.4 for the fifo jobserver, Phase 176) ---
    make_bin="third-party/make/make"
    if [ -x "$make_bin" ] && "$make_bin" --version 2>/dev/null | head -1 | grep -q "{{MAKE_VERSION}}"; then
        echo "  [OK] make {{MAKE_VERSION}} (third-party/make; fifo jobserver)"
    else
        echo "  [MISSING] make {{MAKE_VERSION}} (run: just workspace install-make)"
        fail=1
    fi

    # --- rust-pinned-toolchains ---
    # Every `rust-toolchain.toml` checked into the repo must have its
    # channel installed, together with the components it declares.
    # This is the single source of truth for both versions and components
    # — no separate `rustup toolchain install` / `rustup component add`
    # steps.
    shopt -s globstar nullglob 2>/dev/null
    pinned_missing=()
    comp_missing=()
    rustup_timeout=()
    # Phase 208.D.6 — wrap every `rustup` invocation in `timeout 5s` +
    # disable rustup self-update so a stale rustup or a flaky/slow
    # rustup-dist mirror can't wedge `just doctor` (the audit blocker
    # P13). If a single call exceeds 5 s, doctor reports
    # `rust-pinned-toolchains: rustup unresponsive` and moves on
    # instead of hanging.
    export RUSTUP_NO_SELF_UPDATE=1
    has_component() {
        timeout 5s rustup component list --installed --toolchain "$2" 2>/dev/null \
            | grep -qE "^$1(-[^[:space:]]+)?$"
    }
    for f in $(just workspace _pinned-toolchain-files); do
        channel=$(awk '/^channel *=/ { gsub(/["]/, "", $3); print $3; exit }' "$f")
        [ -z "$channel" ] && continue
        list_out="$(timeout 5s rustup toolchain list 2>/dev/null)"
        rustup_rc=$?
        if [ $rustup_rc -eq 124 ]; then
            rustup_timeout+=("$channel ($f)")
            continue
        fi
        if ! echo "$list_out" | grep -q "^$channel"; then
            pinned_missing+=("$channel ($f)")
            continue
        fi
        components=$(awk '/^components *=/ { sub(/^[^=]*= */, ""); gsub(/[][",]/, " "); print; exit }' "$f")
        for c in $components; do
            [ -z "$c" ] && continue
            has_component "$c" "$channel" || comp_missing+=("$channel/$c")
        done
    done
    if [ ${#pinned_missing[@]} -eq 0 ] && [ ${#comp_missing[@]} -eq 0 ] && [ ${#rustup_timeout[@]} -eq 0 ]; then
        echo "  [OK] rust-pinned-toolchains"
    else
        if [ ${#pinned_missing[@]} -gt 0 ]; then
            echo "  [MISSING] rust-pinned-toolchains channels: ${pinned_missing[*]}"
        fi
        if [ ${#comp_missing[@]} -gt 0 ]; then
            echo "  [MISSING] rust-pinned-toolchains components: ${comp_missing[*]}"
        fi
        if [ ${#rustup_timeout[@]} -gt 0 ]; then
            echo "  [TIMEOUT] rust-pinned-toolchains: rustup unresponsive for ${rustup_timeout[*]}"
            echo "            run: rustup self update; then re-run \`just doctor\`"
        fi
        echo "            run: just workspace rust-pinned-toolchains"
        fail=1
    fi

    # --- rust-targets ---
    tgt_missing=()
    for t in thumbv7em-none-eabihf thumbv7m-none-eabi armv7r-none-eabihf riscv32imc-unknown-none-elf riscv64gc-unknown-none-elf; do
        timeout 5s rustup target list --installed 2>/dev/null | grep -q "^$t$" || tgt_missing+=("$t")
    done
    if [ ${#tgt_missing[@]} -eq 0 ]; then
        echo "  [OK] rust-targets"
    else
        echo "  [MISSING] rust-targets: ${tgt_missing[*]} (run: just workspace rust-targets)"
        fail=1
    fi

    # --- cargo-tools ---
    # cargo-installed tools (nextest / llvm-cov / espflash) land in ~/.cargo/bin,
    # which rustup adds to PATH on install. `nros` is different: Phase 218
    # moved the CLI in-tree at `packages/cli/target/release/nros` (preferred)
    # with `~/.nros/bin/nros` as the transitional fallback. The user may not
    # yet have either path on PATH. Recognise each state separately so we can
    # offer a tailored hint (`source ./activate.sh` for the in-tree case,
    # `export PATH=…` for the transitional case) instead of looping on
    # `just workspace cargo-tools` → install → already-installed short-circuit.
    tool_missing=()
    for t in cargo-nextest cargo-llvm-cov espflash; do
        check command -v "$t" || tool_missing+=("$t")
    done
    nros_path_status="ok"
    nros_resolved=""
    nros_intree="{{justfile_directory()}}/packages/cli/target/release/nros"
    # Canonicalise the in-tree path so we can compare against `which nros`
    # after symlink resolution (e.g. the user might have a symlink in
    # ~/.local/bin → packages/cli/target/release/nros).
    if [ -x "$nros_intree" ]; then
        nros_intree_real="$(readlink -f "$nros_intree" 2>/dev/null || echo "$nros_intree")"
    else
        nros_intree_real=""
    fi
    if command -v nros >/dev/null 2>&1; then
        nros_resolved="$(command -v nros)"
        nros_resolved_real="$(readlink -f "$nros_resolved" 2>/dev/null || echo "$nros_resolved")"
        # Phase 220.A.1 — FAIL on stale shadow. If the resolved `nros` on
        # PATH is anything OTHER than the in-tree CLI binary, the user
        # almost certainly has a pre-218 `~/.cargo/bin/nros` (cargo
        # install) or `~/.nros/bin/nros` (deleted install-nros.sh)
        # outranking the post-218 in-tree binary. Stale CLIs emit
        # pre-K.7.1.b `[patch.crates-io]` blocks that break native rust
        # cyclonedds examples — a silent-but-broken state we want to
        # surface loudly at `just doctor` time.
        if [ -n "$nros_intree_real" ] && [ "$nros_resolved_real" = "$nros_intree_real" ]; then
            :  # resolved to the in-tree CLI — OK
        elif [ -n "$nros_intree_real" ]; then
            nros_path_status="stale-shadow"
        else
            # in-tree CLI not built yet; whatever's on PATH is the only
            # `nros` the user has. We can't classify it as a shadow
            # because we have nothing to shadow. Leave as "ok" — the
            # user is on the transitional / external-install path.
            :
        fi
    elif [ -x "$nros_intree" ]; then
        nros_path_status="needs-activate"
    elif [ -x "${NROS_HOME:-$HOME/.nros}/bin/nros" ]; then
        nros_path_status="needs-path"
    else
        nros_path_status="missing"
    fi

    if [ ${#tool_missing[@]} -eq 0 ] && [ "$nros_path_status" = "ok" ]; then
        echo "  [OK] cargo-tools"
    else
        if [ "$nros_path_status" = "stale-shadow" ]; then
            echo "  [FAIL] nros on PATH ($nros_resolved) shadows the in-tree CLI" \
                 "($nros_intree)."
            echo "         A stale pre-218 \`nros\` emits outdated"
            echo "         \`[patch.crates-io]\` blocks (from \`nros ws sync\`) that"
            echo "         break native rust cyclonedds examples and other downstream"
            echo "         consumers. Clean it up:"
            echo "             rm -f \"\$HOME/.cargo/bin/nros\" \"\$HOME/.nros/bin/nros\""
            echo "             source ./activate.sh"
            echo "         (Or whatever path \`which nros\` reports above, if"
            echo "         neither cargo-bin nor ~/.nros/bin is the source.)"
            fail=1
        elif [ "$nros_path_status" = "needs-activate" ]; then
            echo "  [PATH] nros built at packages/cli/target/release/nros but not on PATH —"
            echo "         run: source ./activate.sh  (or direnv allow / source ./activate.fish)"
            fail=1
        elif [ "$nros_path_status" = "needs-path" ]; then
            echo "  [PATH] nros at ~/.nros/bin/nros (transitional) but not on PATH —"
            echo "         add: export PATH=\"\$HOME/.nros/bin:\$PATH\"  (or migrate to 'just setup-cli')"
            fail=1
        elif [ "$nros_path_status" = "missing" ]; then
            tool_missing+=("nros")
        fi
        if [ ${#tool_missing[@]} -gt 0 ]; then
            echo "  [MISSING] cargo-tools: ${tool_missing[*]} (run: just workspace cargo-tools)"
            fail=1
        fi
    fi

    # --- toolchain env vars (exported by .envrc with sensible defaults) ---
    env_missing=()
    for v in NUTTX_DIR THREADX_DIR NETX_DIR FREERTOS_DIR LWIP_DIR; do
        if [ -z "${!v:-}" ]; then
            env_missing+=("$v")
        fi
    done
    if [ ${#env_missing[@]} -eq 0 ]; then
        echo "  [OK] toolchain env vars exported"
    else
        echo "  [INFO] toolchain env vars not in shell: ${env_missing[*]}"
        echo "        Run 'direnv allow' or 'source .envrc' to export defaults."
    fi

    exit $fail

# =============================================================================
# Sub-recipes — each can be run individually
# =============================================================================

# Install required system packages via apt (requires sudo).
[group("setup")]
apt-packages:
    #!/usr/bin/env bash
    set -e
    pkgs=()
    check_apt() {
        if command -v "$2" &>/dev/null; then
            printf "  %-40s %s\n" "$1" "[already installed]"
        else
            pkgs+=("$1")
        fi
    }
    check_apt gcc-arm-none-eabi              arm-none-eabi-gcc
    check_apt qemu-system-arm                qemu-system-arm
    check_apt qemu-system-misc               qemu-system-riscv64
    check_apt cmake                          cmake
    check_apt socat                          socat
    check_apt gcc-riscv64-unknown-elf        riscv64-unknown-elf-gcc
    if [ -f /usr/include/mbedtls/ssl.h ]; then
        printf "  %-40s %s\n" "libmbedtls-dev" "[already installed]"
    else
        pkgs+=(libmbedtls-dev)
    fi
    # picolibc: check via sysroot
    picolibc_found=false
    if command -v riscv64-unknown-elf-gcc &>/dev/null; then
        sysroot=$(riscv64-unknown-elf-gcc -march=rv32imc -mabi=ilp32 --specs=picolibc.specs -print-sysroot 2>/dev/null || true)
        if [ -n "$sysroot" ] && [ -d "$sysroot/include" ]; then
            picolibc_found=true
        elif [ -d "/usr/lib/picolibc/riscv64-unknown-elf/include" ]; then
            picolibc_found=true
        fi
    fi
    if $picolibc_found; then
        printf "  %-40s %s\n" "picolibc-riscv64-unknown-elf" "[already installed]"
    else
        pkgs+=(picolibc-riscv64-unknown-elf)
    fi
    # kconfig-frontends-nox: required by NuttX build
    if command -v kconfig-conf &>/dev/null || command -v olddefconfig &>/dev/null; then
        printf "  %-40s %s\n" "kconfig-frontends-nox" "[already installed]"
    else
        pkgs+=(kconfig-frontends-nox)
    fi
    if [ ${#pkgs[@]} -eq 0 ]; then
        echo "All system packages already installed."
    else
        echo "Installing: ${pkgs[*]}"
        sudo apt-get install -y "${pkgs[@]}"
    fi

# Print the list of project-owned `rust-toolchain.toml` files that
# `rust-pinned-toolchains` and `doctor` walk. Third-party vendored
# files (third-party/, scripts/zenohd/) are excluded. Private helper —
# kept as a recipe so the list has a single definition.
[private]
_pinned-toolchain-files:
    #!/usr/bin/env bash
    set -e
    # Phase 208.D.6 — replaced `shopt -s globstar; for f in
    # examples/**/rust-toolchain.toml` with a `find -prune` because
    # bash globstar descends into every `target/`, `target-*/`,
    # `build-*/` dir under `examples/` (which is GB of build output
    # on a warm tree) and took ~78 s on this host — the symptom the
    # audit reported as "`just doctor tier=default` hangs". The find
    # form prunes those dirs and returns in ~10 ms.
    emit() { [ -f "$1" ] && echo "$1"; }
    emit rust-toolchain.toml
    emit tools/rust-toolchain.toml
    emit packages/verification/nros-verification/rust-toolchain.toml
    find examples \
        -type d \( -name target -o -name 'target-*' -o -name 'build-*' -o -name build \) -prune \
        -o -name rust-toolchain.toml -print 2>/dev/null

# Install every pinned toolchain referenced by a checked-in
# `rust-toolchain.toml`, together with the components each file declares.
#
# This is the single source of truth for Rust toolchain versions in the
# repo:
#   - rust-toolchain.toml                                   stable + workspace components
#   - tools/rust-toolchain.toml                             pinned nightly for workspace tooling
#   - examples/qemu-arm-nuttx/rust-toolchain.toml           nightly for -Z build-std + patched libc
#   - examples/zephyr/rust-toolchain.toml                   nightly for Corrosion + Zephyr Rust
#   - packages/verification/nros-verification/…             Verus-required channel
#
# Channels and components are never duplicated into shell scripts; the
# TOML files are authoritative. Running cargo in a directory triggers
# rustup's auto-download of the channel, but *not* of declared
# components — so this recipe installs both explicitly.
[group("setup")]
rust-pinned-toolchains:
    #!/usr/bin/env bash
    set -e
    declare -A seen
    # Split installs into two steps:
    #   1. `rustup toolchain install --profile minimal <channel>` — idempotent;
    #      skips when the toolchain is already present.
    #   2. `rustup component add <c>` per declared component — also idempotent;
    #      `rustup component add` reports "already installed" and exits 0 when
    #      the component is present.
    #
    # The two-step form replaces the prior `rustup toolchain install … -c <c>`
    # one-liner, which forces a whole-toolchain re-validation. On a tree where
    # someone has previously added a non-stock target via `rustup target add`
    # (e.g. armv7r-none-eabihf via `just workspace rust-targets`) the
    # re-validation tries to roll back changes that reference a manifest the
    # local toolchain never had (`manifest-rust-std-thumbv7em-none-eabi`
    # without the `hf` suffix, etc.) — the install then aborts mid-rollback
    # and leaves rustup state partially uninstalled. `rustup component add`
    # is per-component and self-healing, so a single missing component is
    # the only thing it touches.
    while read -r f; do
        [ -z "$f" ] && continue
        channel=$(awk '/^channel *=/ { gsub(/["]/, "", $3); print $3; exit }' "$f")
        components=$(awk '/^components *=/ { sub(/^[^=]*= */, ""); gsub(/[][",]/, " "); print; exit }' "$f")
        [ -z "$channel" ] && continue
        key="$channel | $components"
        if [ -n "${seen[$key]}" ]; then continue; fi
        seen[$key]=1
        echo "=== $channel ($f; components: ${components:-<none>}) ==="
        rustup toolchain install --profile minimal "$channel"
        for c in $components; do
            [ -z "$c" ] && continue
            rustup component add --toolchain "$channel" "$c"
        done
    done < <(just workspace _pinned-toolchain-files)

# Install cross-compilation targets.
[group("setup")]
rust-targets:
    #!/usr/bin/env bash
    set -e
    rustup target add thumbv7em-none-eabihf
    rustup target add thumbv7m-none-eabi
    # Phase 100 — AGX Orin SPE (Cortex-R5F). Tier 3, but rustup ships a
    # prebuilt rust-std for both stable and nightly so `cargo check`
    # works without `-Zbuild-std`. The SPE board crate (100.6) may
    # still opt into `-Zbuild-std` for `panic_abort` / build-std-features
    # tuning; that lives in the board crate's .cargo/config.toml.
    rustup target add armv7r-none-eabihf
    rustup target add riscv32imc-unknown-none-elf
    rustup target add riscv64gc-unknown-none-elf
    rustup +{{NIGHTLY}} target add thumbv7m-none-eabi
    if rustc +{{NIGHTLY}} --print target-list 2>/dev/null | grep -q armv7a-nuttx-eabi; then
        echo "  armv7a-nuttx-eabi (NuttX Tier 3): supported via nightly + build-std"
    else
        echo "  WARNING: armv7a-nuttx-eabi not in nightly target list — NuttX builds may fail"
    fi

# Install cargo tools (nextest, llvm-cov, espflash, nano-ros CLI).
[group("setup")]
cargo-tools:
    #!/usr/bin/env bash
    set -e
    cargo install cargo-nextest --locked
    cargo install cargo-llvm-cov --locked
    cargo install espflash --locked      || echo "WARNING: espflash install failed (non-fatal)"
    cargo install rustfilt --locked      || echo "WARNING: rustfilt install failed (non-fatal)"
    cargo install cargo-show-asm --locked || echo "WARNING: cargo-show-asm install failed (non-fatal)"
    # Phase 218 — `nros` lives in-tree at `packages/cli/` (the merge
    # back into nano-ros). Build from source via `just setup-cli`
    # (idempotent: skips if the binary is fresh relative to
    # packages/cli/Cargo.lock). Contributors hacking on nros itself
    # still have `$NROS_CLI` as the override knob.
    just setup-cli
