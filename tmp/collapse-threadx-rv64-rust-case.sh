#!/usr/bin/env bash
# Phase 118.B.6 — collapse qemu-riscv64-threadx/rust/<rmw>/<case>
# into qemu-riscv64-threadx/rust/<case>.
set -euo pipefail

case_name="${1:?usage: $0 <case>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
src_zenoh="${root}/examples/qemu-riscv64-threadx/rust/zenoh/${case_name}"
src_dds="${root}/examples/qemu-riscv64-threadx/rust/dds/${case_name}"
dst="${root}/examples/qemu-riscv64-threadx/rust/${case_name}"

[ -d "$src_zenoh" ] || { echo "missing $src_zenoh" >&2; exit 1; }
[ -d "$dst" ] && { echo "already exists: $dst"; exit 0; }

has_dds="no"
[ -d "$src_dds" ] && has_dds="yes"

mkdir -p "$dst"
cp -r "$src_zenoh/src" "$dst/"
[ -f "$src_zenoh/config.toml" ] && cp "$src_zenoh/config.toml" "$dst/"
[ -f "$src_zenoh/package.xml" ] && cp "$src_zenoh/package.xml" "$dst/"
[ -d "$src_zenoh/generated" ] && cp -r "$src_zenoh/generated" "$dst/"
[ -d "$src_zenoh/.cargo" ] && cp -r "$src_zenoh/.cargo" "$dst/"

if [ -f "$dst/.cargo/config.toml" ]; then
    sed -i 's|../../../../..|../../../..|g' "$dst/.cargo/config.toml"
fi

pkg_name=$(awk -F\" '/^name = "/{print $2; exit}' "$src_zenoh/Cargo.toml")
bin_name=$(awk -F\" '/^name = "/{n++; if (n==2) {print $2; exit}}' "$src_zenoh/Cargo.toml")
[ -z "$bin_name" ] && bin_name="$pkg_name"

msg_dep="std_msgs"
grep -q "example_interfaces" "$src_zenoh/Cargo.toml" 2>/dev/null && msg_dep="example_interfaces"

dds_feature=""
dds_dep=""
crit_dep=""
plat_dep=""
if [ "$has_dds" = "yes" ]; then
    dds_feature='rmw-dds   = ["dep:nros-rmw-dds", "dep:nros-platform-critical-section", "nros/alloc"]'
    dds_dep='nros-rmw-dds = { path = "../../../../packages/dds/nros-rmw-dds", features = ["platform-threadx"], optional = true }'
    crit_dep='nros-platform-critical-section = { path = "../../../../packages/core/nros-platform-critical-section", optional = true }'
    plat_dep='nros-platform = { path = "../../../../packages/core/nros-platform", default-features = false, features = ["platform-threadx", "global-allocator", "critical-section"] }'
fi

cat > "$dst/Cargo.toml" <<TOML
[package]
name = "${pkg_name}"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
publish = false
description = "${case_name} on QEMU RISC-V ThreadX (Phase 118 collapsed)"

[[bin]]
name = "${bin_name}"
test = false
bench = false

[features]
default = ["rmw-zenoh"]
rmw-zenoh = ["dep:nros-rmw-zenoh"]
${dds_feature}

[dependencies]
nros-board-threadx-qemu-riscv64 = { path = "../../../../packages/boards/nros-board-threadx-qemu-riscv64" }
nros = { path = "../../../../packages/core/nros", default-features = false, features = ["rmw-cffi", "platform-threadx", "ros-humble"] }
${plat_dep}
${msg_dep} = { version = "*", default-features = false }

nros-rmw-zenoh = { path = "../../../../packages/zpico/nros-rmw-zenoh", features = ["platform-threadx", "ros-humble"], optional = true }
${dds_dep}
${crit_dep}
TOML

cat > "$dst/.gitignore" <<'IGN'
/target/
/target-zenoh/
/target-dds/
/generated/
/Cargo.lock
IGN

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

lines = text.splitlines(keepends=True)
last_use_idx = -1
for i, ln in enumerate(lines):
    if ln.lstrip().startswith("use "):
        last_use_idx = i
insert_at = (last_use_idx + 1) if last_use_idx >= 0 else 0
lines.insert(insert_at, extern_blocks + compile_err + "\n" + helper)
text = "".join(lines)

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
