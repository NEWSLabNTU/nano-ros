#!/usr/bin/env bash
# Phase 118.B.1 — collapse one native/rust/<rmw>/<case> sibling into
# native/rust/<case>. Idempotent: skips if the target dir already exists.
#
# Usage: tmp/collapse-native-rust-case.sh <case>
set -euo pipefail

case_name="${1:?usage: $0 <case>}"
root="$(cd "$(dirname "$0")/.." && pwd)"
src="${root}/examples/native/rust/zenoh/${case_name}"
dst="${root}/examples/native/rust/${case_name}"

if [ ! -d "$src" ]; then
    echo "missing source: $src" >&2
    exit 1
fi
if [ -d "$dst" ]; then
    echo "already exists: $dst — skipping"
    exit 0
fi

mkdir -p "$dst"
cp -r "$src/src" "$dst/"
cp "$src/package.xml" "$dst/"
[ -d "$src/.cargo" ] && cp -r "$src/.cargo" "$dst/"

# .cargo/config.toml: paths 5→4 segments
if [ -f "$dst/.cargo/config.toml" ]; then
    sed -i 's|../../../../..|../../../..|g' "$dst/.cargo/config.toml"
fi

# Cargo.toml rewrite: extract per-case [package] name + [[bin]] then
# splice in the collapsed-shape body. The binary name comes from
# the [[bin]] section of the zenoh sibling.
pkg_name=$(awk -F\" '/^name = "/{print $2; exit}' "$src/Cargo.toml")
bin_name=$(awk -F\" '/^name = "/{n++; if (n==2) {print $2; exit}}' "$src/Cargo.toml")
if [ -z "$bin_name" ]; then
    # Fall back to the case name (e.g. service-server)
    bin_name="$case_name"
fi

cat > "$dst/Cargo.toml" <<TOML
[package]
name = "${pkg_name}"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
publish = false

[[bin]]
name = "${bin_name}"
path = "src/main.rs"

[features]
# Phase 118 — RMW selected at build time via mutually exclusive features.
default = ["rmw-zenoh"]
rmw-zenoh = ["dep:nros-rmw-zenoh"]
rmw-dds   = ["dep:nros-rmw-dds"]
rmw-xrce  = ["dep:nros-rmw-xrce-cffi"]

[dependencies]
nros = { path = "../../../../packages/core/nros", default-features = false, features = ["std", "rmw-cffi", "platform-posix"] }
nros-platform-cffi = { path = "../../../../packages/core/nros-platform-cffi", features = ["posix-c-port"] }
std_msgs = { version = "*", default-features = false }
log = "0.4"
env_logger = "0.11"

nros-rmw-zenoh    = { path = "../../../../packages/zpico/nros-rmw-zenoh", features = ["std", "platform-posix", "ros-humble"], optional = true }
nros-rmw-dds      = { path = "../../../../packages/dds/nros-rmw-dds",    features = ["platform-posix"], optional = true }
nros-rmw-xrce-cffi = { path = "../../../../packages/xrce/nros-rmw-xrce-cffi", optional = true }
TOML

# .gitignore
cat > "$dst/.gitignore" <<'IGN'
/target/
/target-zenoh/
/target-dds/
/target-xrce/
/generated/
/Cargo.lock
IGN

# Re-write each `nros_rmw_*::register().expect(...)` call to use the
# new `register_rmw()` helper, and inject the helper at the top of
# main.rs (right after the last `use` statement before the first
# `fn` — easiest robust insertion point is right after `use std_msgs`).
python3 - "$dst/src/main.rs" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text()

helper = '''
// Phase 118 — RMW selection is build-time via mutually exclusive
// `rmw-{zenoh,dds,xrce}` features. `register_rmw()` fans out under
// `#[cfg(feature)]`; the rest of the file stays RMW-agnostic.

#[cfg(not(any(feature = "rmw-zenoh", feature = "rmw-dds", feature = "rmw-xrce")))]
compile_error!(
    "this example requires exactly one of `rmw-zenoh`, `rmw-dds`, or `rmw-xrce`",
);

fn register_rmw() -> Result<(), &'static str> {
    #[cfg(feature = "rmw-zenoh")]
    { nros_rmw_zenoh::register().map_err(|_| "zenoh register failed")?; }
    #[cfg(feature = "rmw-dds")]
    { nros_rmw_dds::register().map_err(|_| "dds register failed")?; }
    #[cfg(feature = "rmw-xrce")]
    { nros_rmw_xrce_cffi::register().map_err(|_| "xrce register failed")?; }
    Ok(())
}
'''

# Insert helper after the final `use ...;` line preceding the first `fn`.
lines = text.splitlines(keepends=True)
last_use = -1
first_fn = None
for i, ln in enumerate(lines):
    stripped = ln.lstrip()
    if stripped.startswith("use "):
        last_use = i
    if first_fn is None and (stripped.startswith("fn ") or stripped.startswith("pub fn ") or stripped.startswith("#[")):
        first_fn = i
        break

insert_at = (last_use + 1) if last_use >= 0 else 0
lines.insert(insert_at, helper)
text = "".join(lines)

# Replace per-RMW register calls.
text = re.sub(
    r"nros_rmw_(?:zenoh|dds|xrce_cffi)::register\(\)\.expect\([^)]*\);",
    'register_rmw().expect("Failed to register RMW backend");',
    text,
)

path.write_text(text)
PY

echo "collapsed: $dst"
