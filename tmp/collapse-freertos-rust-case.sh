#!/usr/bin/env bash
# Phase 118.B.4 — collapse one qemu-arm-freertos/rust/<rmw>/<case>
# sibling into qemu-arm-freertos/rust/<case>. If a dds sibling exists
# the dst Cargo.toml exposes both `rmw-zenoh` + `rmw-dds`; otherwise
# only `rmw-zenoh`.
#
# Usage: tmp/collapse-freertos-rust-case.sh <case>
set -euo pipefail

case_name="${1:?usage: $0 <case>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
src_zenoh="${root}/examples/qemu-arm-freertos/rust/zenoh/${case_name}"
src_dds="${root}/examples/qemu-arm-freertos/rust/dds/${case_name}"
dst="${root}/examples/qemu-arm-freertos/rust/${case_name}"

if [ ! -d "$src_zenoh" ]; then
    echo "missing zenoh source: $src_zenoh" >&2
    exit 1
fi
if [ -d "$dst" ]; then
    echo "already exists: $dst — skipping"
    exit 0
fi

has_dds="no"
[ -d "$src_dds" ] && has_dds="yes"

mkdir -p "$dst"
cp -r "$src_zenoh/src" "$dst/"
[ -f "$src_zenoh/config.toml" ] && cp "$src_zenoh/config.toml" "$dst/"
[ -f "$src_zenoh/package.xml" ] && cp "$src_zenoh/package.xml" "$dst/"
[ -f "$src_zenoh/memory.x" ] && cp "$src_zenoh/memory.x" "$dst/"
[ -f "$src_zenoh/build.rs" ] && cp "$src_zenoh/build.rs" "$dst/"
[ -d "$src_zenoh/generated" ] && cp -r "$src_zenoh/generated" "$dst/"
[ -d "$src_zenoh/.cargo" ] && cp -r "$src_zenoh/.cargo" "$dst/"

# Fix .cargo/config.toml path depth: 5 segments → 4
if [ -f "$dst/.cargo/config.toml" ]; then
    sed -i 's|../../../../..|../../../..|g' "$dst/.cargo/config.toml"
fi

pkg_name=$(awk -F\" '/^name = "/{print $2; exit}' "$src_zenoh/Cargo.toml")
bin_name=$(awk -F\" '/^name = "/{n++; if (n==2) {print $2; exit}}' "$src_zenoh/Cargo.toml")
[ -z "$bin_name" ] && bin_name="$pkg_name"

# Detect message dep — std_msgs vs example_interfaces
msg_dep="std_msgs"
if grep -q "example_interfaces" "$src_zenoh/Cargo.toml" 2>/dev/null; then
    msg_dep="example_interfaces"
fi

dds_feature_line=""
dds_dep_line=""
crit_dep_line=""
if [ "$has_dds" = "yes" ]; then
    dds_feature_line='rmw-dds   = ["dep:nros-rmw-dds", "dep:nros-platform-critical-section"]'
    dds_dep_line='nros-rmw-dds   = { path = "../../../../packages/dds/nros-rmw-dds",    features = ["platform-freertos"], optional = true }'
    crit_dep_line='nros-platform-critical-section = { path = "../../../../packages/core/nros-platform-critical-section", optional = true }'
fi

cat > "$dst/Cargo.toml" <<TOML
[package]
name = "${pkg_name}"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
publish = false
description = "${case_name} on QEMU MPS2-AN385 with FreeRTOS + lwIP (Phase 118 collapsed)"

[[bin]]
name = "${bin_name}"
test = false
bench = false

[features]
default = ["rmw-zenoh"]
rmw-zenoh = ["dep:nros-rmw-zenoh", "nros-board-mps2-an385-freertos/rmw-zenoh"]
${dds_feature_line}

[dependencies]
nros-board-mps2-an385-freertos = { path = "../../../../packages/boards/nros-board-mps2-an385-freertos", default-features = false }
nros = { path = "../../../../packages/core/nros", default-features = false, features = ["rmw-cffi", "platform-freertos", "ros-humble"] }
nros-platform = { path = "../../../../packages/core/nros-platform", default-features = false, features = ["platform-freertos", "global-allocator"] }
${msg_dep} = { version = "*", default-features = false }
panic-semihosting = { version = "0.6", features = ["exit"] }

nros-rmw-zenoh = { path = "../../../../packages/zpico/nros-rmw-zenoh", features = ["platform-freertos", "ros-humble"], optional = true }
${dds_dep_line}
${crit_dep_line}
TOML

cat > "$dst/.gitignore" <<'IGN'
/target/
/target-zenoh/
/target-dds/
/generated/
/Cargo.lock
IGN

# main.rs: only inject the cfg-dispatched register helper if it
# doesn't already exist (e.g. talker was hand-edited in 118.B.4
# PoC). The helper is no-op if it's already present.
if ! grep -q "register_rmw" "$dst/src/main.rs"; then
    python3 - "$dst/src/main.rs" "$has_dds" <<'PY'
import sys
from pathlib import Path

src = Path(sys.argv[1])
has_dds = sys.argv[2] == "yes"
text = src.read_text()

if has_dds:
    extern_blocks = '''
#[cfg(feature = "rmw-dds")]
extern crate alloc;
#[cfg(feature = "rmw-dds")]
extern crate nros_platform_critical_section as _;

'''
    compile_err = '''#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-dds")))]
compile_error!("this example requires `rmw-zenoh` or `rmw-dds`");
'''
    helper = '''fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    { nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?; }
    #[cfg(feature = "rmw-dds")]
    { nros_rmw_dds::register().map_err(|_| "dds register failed")?; }
    Ok(())
}

'''
else:
    extern_blocks = ""
    compile_err = '''#[cfg(not(feature = "rmw-zenoh"))]
compile_error!("this example requires `rmw-zenoh`");
'''
    helper = '''fn register_rmw() -> Result<(), &'static str> {
    nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")
}

'''

# Locate insertion point: right after the last `use ...;` line
# at the top of the file.
lines = text.splitlines(keepends=True)
last_use_idx = -1
for i, ln in enumerate(lines):
    stripped = ln.lstrip()
    if stripped.startswith("use "):
        last_use_idx = i
insert_at = (last_use_idx + 1) if last_use_idx >= 0 else 0

block = extern_blocks + compile_err + "\n" + helper
lines.insert(insert_at, block)
text = "".join(lines)

# Swap per-RMW register calls to the helper.
text = text.replace(
    'nros_rmw_zenoh::register().expect("Failed to register RMW backend");',
    'register_rmw().expect("Failed to register RMW backend");',
)
text = text.replace(
    'nros_rmw_dds::register().expect("Failed to register RMW backend");',
    'register_rmw().expect("Failed to register RMW backend");',
)

src.write_text(text)
PY
fi

echo "collapsed: $dst (dds=${has_dds})"
