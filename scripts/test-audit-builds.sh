#!/usr/bin/env bash
# Phase 179.F — find build-tool invocations that still run from tests.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

scopes=(
  packages/testing/nros-tests/src
  packages/testing/nros-tests/tests
  packages/codegen/packages/nros-cli-core/tests
)

echo "== direct build-tool process spawns =="
rg -n \
  'Command::new\("(cargo|cmake|make|ninja|west|just|idf\.py)"\)|Command::new\(compiler\)|build::run\(|build_generated_package\(' \
  "${scopes[@]}" || true

echo
echo "== shell command strings that mention build tools =="
rg -n \
  '(cargo|cmake|make|ninja|west|just|idf\.py) (build|--build|run|test|nextest|setup|list|--version)' \
  "${scopes[@]}" || true

echo
echo "== fixture resolver names that look like builders =="
rg -n \
  '\bbuild_[a-zA-Z0-9_]+\(' \
  packages/testing/nros-tests/tests packages/testing/nros-tests/src/fixtures \
  | rg -v 'src/fixtures/binaries|fn build_|let build_|let build_dir|build_dir\(|build_dir\.|builder' || true

cat <<'EOF'

Review notes:
- `build_*` helpers under `packages/testing/nros-tests/src/fixtures/binaries/`
  are expected to resolve prebuilt artifacts and should not invoke cargo,
  CMake, west, or make.
- Expensive runtime fixtures should be staged by `just build-test-fixtures`.
- Tests may still invoke build tools when the build action is the product
  under test, for example source-distribution CMake smoke tests or
  orchestration CLI tests.
EOF
