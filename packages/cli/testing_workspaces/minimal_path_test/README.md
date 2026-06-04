# Minimal Path Test

## Purpose

This is a minimal test case to investigate Rust's `#[path]` attribute behavior with nested inline modules.

## ✅ SOLUTION FOUND

### The Problem

When using nested inline modules with `#[path]` attributes:

```rust
pub mod msg {
    pub mod rmw {
        #[path = "bool_rmw.rs"]
        pub mod bool;
    }
}
```

Rust treats the inline `rmw` module as if it has a **virtual directory** at `src/msg/rmw/`, even though `rmw` is declared inline (not as a separate file).

### The Solution

**Files must be placed in directories that match the nested module structure:**

```
src/
├── lib.rs                    # Declares: pub mod msg { pub mod rmw { } }
└── msg/
    ├── bool_idiomatic.rs     # For msg module
    └── rmw/                  # Directory for nested rmw module
        └── bool_rmw.rs       # For msg::rmw module
```

With this structure, the simple path attribute works:

```rust
pub mod msg {
    pub mod rmw {
        #[path = "bool_rmw.rs"]  // ✅ Works! Looks in src/msg/rmw/
        pub mod bool;
    }

    #[path = "bool_idiomatic.rs"]  // ✅ Works! Looks in src/msg/
    pub mod bool;
}
```

## Key Insight

**Inline modules create virtual directory contexts.** Even though `rmw` is declared inline within `msg` in `lib.rs`, Rust treats it as if `rmw` exists at `src/msg/rmw/` for the purpose of resolving `#[path]` attributes.

## cargo-ros2-bindgen Fix

The generator needs to:
1. Create `src/msg/rmw/` directory for RMW files
2. Create `src/srv/rmw/` directory for service RMW files
3. Create `src/action/rmw/` directory for action RMW files
4. Keep idiomatic files in the parent directories (`src/msg/`, `src/srv/`, `src/action/`)

## Test Results

```bash
$ cargo build
   Compiling minimal_path_test v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.04s
```

✅ Build succeeds with correct directory structure!
