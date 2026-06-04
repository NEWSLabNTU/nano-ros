#!/usr/bin/env bash
set -e

source scripts/build/cargo.sh
source scripts/build/generate-rust-incremental.sh

NROS="$(nros_cli_bin)"
echo "Refreshing Rust bindings..."

for pkg in $(find examples -name package.xml -not -path '*/target/*' -not -path '*/generated/*' | sort); do
    dir="$(dirname "$pkg")"
    nros_generate_rust_if_needed "$dir" "$NROS"
done

for pkg in $(find packages/testing/nros-bench packages/testing/nros-tests/bins packages/testing/nros-smoke \
                 -name package.xml -not -path '*/target/*' -not -path '*/generated/*' 2>/dev/null | sort); do
    dir="$(dirname "$pkg")"
    nros_generate_rust_if_needed "$dir" "$NROS"
done

echo "All bindings refreshed!"
