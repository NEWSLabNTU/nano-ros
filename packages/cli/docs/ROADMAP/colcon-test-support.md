# Feature: colcon test Support

**Goal**: Implement full `colcon test` support for Rust ROS 2 packages.

**Priority**: High (enables standard ROS 2 testing workflow)

**Status**: 🔄 In Progress

---

## Overview

The `colcon test` command is the standard way to run tests in ROS 2 workspaces. Currently, the test task is registered but has a minimal implementation that doesn't properly integrate with the colcon test framework.

**Current State**:
- Entry point registered in `pyproject.toml` ✅
- Basic `AmentCargoTestTask` class exists ✅
- Missing critical features for proper integration ❌

---

## Implementation Tasks

### Task 1: Fix Workspace Config File Usage ⚠️ HIGH PRIORITY

**Problem**: The test task doesn't pass `--config ros2_cargo_config.toml`, causing tests to fail when packages depend on generated ROS message bindings.

**Location**: `packages/colcon-cargo-ros2/colcon_cargo_ros2/task/ament_cargo/test.py`

**Implementation**:
- [ ] Derive `build_base` (parent of package build dir) from `args.build_base`
- [ ] Derive `workspace_root` from build_base
- [ ] Add `--config` flag pointing to `build/ros2_cargo_config.toml`
- [ ] Add `--manifest-path` flag for package Cargo.toml
- [ ] Run cargo from workspace root (like build task does)

**Code Pattern** (from build task):
```python
# Derive workspace paths
build_base = Path(args.build_base).parent
workspace_root = build_base.parent
config_file = build_base / 'ros2_cargo_config.toml'

cmd = ['cargo', 'test']
cmd.extend(['--manifest-path', str(manifest_path)])
cmd.extend(['--config', str(config_file)])
```

**Acceptance Criteria**:
- [ ] Tests can find workspace-generated ROS message bindings
- [ ] Tests pass for packages that use `std_msgs`, `geometry_msgs`, etc.

---

### Task 2: Add Environment Setup

**Problem**: Tests don't have access to dependency environment variables (library paths, ament paths).

**Location**: `packages/colcon-cargo-ros2/colcon_cargo_ros2/task/ament_cargo/test.py`

**Implementation**:
- [ ] Import `get_command_environment` from `colcon_core.shell`
- [ ] Call `get_command_environment('test', args.build_base, self.context.dependencies)`
- [ ] Pass environment to `run()` call

**Code**:
```python
from colcon_core.shell import get_command_environment

try:
    env = await get_command_environment(
        'test', args.build_base, self.context.dependencies)
except RuntimeError as e:
    logger.error(str(e))
    return 1
```

**Acceptance Criteria**:
- [ ] Tests can link against workspace-built libraries
- [ ] `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH` includes dependency paths

---

### Task 3: Add Build Verification

**Problem**: Tests run even if package hasn't been built, causing confusing errors.

**Location**: `packages/colcon-cargo-ros2/colcon_cargo_ros2/task/ament_cargo/test.py`

**Implementation**:
- [ ] Check `os.path.exists(args.build_base)` before running tests
- [ ] Return error with helpful message if not built

**Code**:
```python
if not os.path.exists(args.build_base):
    logger.error(f"Build directory not found: {args.build_base}")
    logger.error("Has this package been built before? Run 'colcon build' first.")
    return 1
```

**Acceptance Criteria**:
- [ ] Clear error message when running `colcon test` without `colcon build`

---

### Task 4: Add TestFailure Event

**Problem**: `colcon test-result` doesn't show failures because no `TestFailure` event is posted.

**Location**: `packages/colcon-cargo-ros2/colcon_cargo_ros2/task/ament_cargo/test.py`

**Implementation**:
- [ ] Import `TestFailure` from `colcon_core.event.test`
- [ ] Post event when `cargo test` returns non-zero
- [ ] Still return 0 (let colcon handle failure aggregation)

**Code**:
```python
from colcon_core.event.test import TestFailure

completed = await run(self.context, cmd, ...)

if completed.returncode:
    self.context.put_event_into_queue(TestFailure(pkg.name))
# Always return 0 - colcon handles failure reporting via events
return 0
```

**Acceptance Criteria**:
- [ ] `colcon test-result` shows which packages failed
- [ ] `--return-code-on-test-failure` flag works correctly

---

### Task 5: Generate JUnit XML Output

**Problem**: `colcon test-result --all` has no results to display.

**Location**: `packages/colcon-cargo-ros2/colcon_cargo_ros2/task/ament_cargo/test.py`

**Implementation Options**:

**Option A: Use cargo's JUnit format (Recommended)**
```python
# Cargo test now supports stable JUnit output
cmd.extend(['--', '--format', 'junit'])
# Capture output and write to file
```

**Option B: Generate XML manually (like colcon-cargo)**
```python
from xml.dom import minidom
import xml.etree.ElementTree as eTree

def _write_test_results(self, path, pkg_name, completed):
    testsuites = eTree.Element('testsuites')
    testsuite = eTree.SubElement(testsuites, 'testsuite',
        {'name': pkg_name, 'tests': '1', 'failures': str(failures)})
    # ... generate XML
    path.write_bytes(minidom.parseString(
        eTree.tostring(testsuites)).toprettyxml(encoding='utf-8'))
```

**Output Location**:
```python
test_result_path = Path(
    args.test_result_base if args.test_result_base
    else args.build_base) / 'cargo_test.xml'
```

**Acceptance Criteria**:
- [ ] `colcon test-result --all` displays test results
- [ ] JUnit XML file created in build directory

---

### Task 6: Add Cargo Format Check (Optional)

**Problem**: No style checking during test phase.

**Reference**: colcon-cargo runs `cargo fmt --check` alongside `cargo test`

**Implementation**:
- [ ] Add `--cargo-fmt-check` argument (default: disabled)
- [ ] Run `cargo fmt --check` when enabled
- [ ] Include result in test report

**Code**:
```python
parser.add_argument(
    '--cargo-fmt-check',
    action='store_true',
    help='Run cargo fmt --check as part of testing')

if args.cargo_fmt_check:
    fmt_completed = await run(
        self.context,
        ['cargo', 'fmt', '--check', '--', '--color=never'],
        cwd=args.path, env=env, capture_output=True)
```

**Acceptance Criteria**:
- [ ] `colcon test --cargo-fmt-check` runs format verification
- [ ] Format failures included in test results

---

### Task 7: Support Retest Options

**Problem**: `--retest-until-pass` and `--retest-until-fail` don't work.

**Reference**: See `colcon-cmake` implementation

**Implementation**:
- [ ] Check `args.retest_until_pass` and `args.retest_until_fail`
- [ ] Loop test execution accordingly
- [ ] For retest-until-pass, only rerun failed tests

**Code**:
```python
rerun = 0
while True:
    completed = await run(self.context, cmd, ...)

    if not completed.returncode:
        break

    if args.retest_until_pass > rerun:
        rerun += 1
        continue

    break
```

**Acceptance Criteria**:
- [ ] `colcon test --retest-until-pass 3` retries failed tests
- [ ] `colcon test --retest-until-fail 3` runs until failure

---

### Task 8: Add Unit Tests

**Location**: `packages/colcon-cargo-ros2/test/test_test.py` (new file)

**Tests to Add**:
- [ ] `test_test_cmd_generation` - Verify command construction
- [ ] `test_config_file_inclusion` - Verify --config flag added
- [ ] `test_environment_setup` - Mock and verify env setup
- [ ] `test_junit_xml_generation` - Verify XML output format
- [ ] `test_test_failure_event` - Verify event posted on failure

---

## Testing

### Manual Testing

```bash
# 1. Build workspace first
cd testing_workspaces/complex_workspace
just clean && just build

# 2. Run tests
colcon test
colcon test-result --all

# 3. Verify specific package
colcon test --packages-select robot_controller
colcon test-result --verbose
```

### Integration Test Workspace

Use `testing_workspaces/complex_workspace` which has:
- Multiple packages with dependencies
- Unit tests in Rust code
- Dependencies on generated message bindings

---

## Dependencies

- `colcon-core` >= 0.5.0 (for TaskExtensionPoint ^1.0)
- Cargo with `--format junit` support (Rust 1.70+)

---

## References

- **colcon-cmake test**: `external/colcon-cmake/colcon_cmake/task/cmake/test.py`
- **colcon-cargo test**: `external/colcon-cargo/colcon_cargo/task/cargo/test.py`
- **colcon-core task API**: `external/colcon-core/colcon_core/task/__init__.py`
- **Research notes**: `tmp/colcon_test_implementation.md`

---

## Timeline

| Task | Priority | Effort | Status |
|------|----------|--------|--------|
| Task 1: Config file | ⚠️ High | 1 hour | ❌ |
| Task 2: Environment | ⚠️ High | 30 min | ❌ |
| Task 3: Build check | Medium | 15 min | ❌ |
| Task 4: TestFailure | ⚠️ High | 30 min | ❌ |
| Task 5: JUnit XML | ⚠️ High | 1 hour | ❌ |
| Task 6: Fmt check | Low | 30 min | ❌ |
| Task 7: Retest | Low | 1 hour | ❌ |
| Task 8: Unit tests | Medium | 2 hours | ❌ |

**Estimated Total**: ~7 hours

**Minimum Viable**: Tasks 1-5 (~3.5 hours)
