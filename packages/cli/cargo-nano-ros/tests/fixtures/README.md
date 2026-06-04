# Test Fixtures

This directory contains test project fixtures used by integration tests.

## Available Fixtures

### `basic_project/`
A minimal Rust project with no dependencies. Used for testing basic workflow and project structure validation.

### `project_with_deps/`
A Rust project with a non-ROS dependency (serde). Used for testing dependency discovery when no ROS packages are present.

## Usage

Tests copy these fixtures to temporary directories and run integration tests against them. This approach ensures:
- Consistent test environments
- No need to dynamically generate test projects in code
- Easy maintenance and updates
- Version control of test fixtures

## Adding New Fixtures

To add a new test fixture:
1. Create a new directory under `tests/fixtures/`
2. Add a valid `Cargo.toml` and `src/main.rs`
3. Update integration tests to use the new fixture with `copy_test_project("fixture_name", dest)`
