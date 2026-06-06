# Workspace Examples

These are product-shaped nano-ros workspaces. They use the package roles
documented in the book:

- `src/*_pkg/`: Node packages with component code only.
- `src/demo_bringup/`: Bringup package with `package.xml`, `system.toml`,
  `launch/`, and optional config files. It has no build file.
- `src/native_entry/`: Entry package with the `main()` for the native target.

Build them with the user workflow:

```bash
source ./activate.sh
cd examples/workspaces/<rust|c|cpp|mixed>
nros setup native
nros ws sync
nros codegen-system --bringup demo_bringup
```

Then use the platform build tool:

```bash
cargo build -p native_entry
# or
cmake -S . -B build && cmake --build build
```

The workspaces currently ship native entries. Additional entry packages should
be added as sibling packages, for example `src/freertos_entry/` or
`src/zephyr_entry/`, while reusing the same Node and Bringup packages.
