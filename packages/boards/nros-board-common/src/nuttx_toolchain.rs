//! phase-548 W3 — the NuttX cross compiler is the PINNED one, with a GCC floor.
//!
//! Every NuttX build script (`nuttx_ffi_build`, `nuttx_platform_build`,
//! `nuttx_image_link`) used to read `NUTTX_CROSS` with a bare
//! `arm-none-eabi-gcc` default, and the FFI build's cc-rs compiles set no
//! compiler at all. So the build used whatever `PATH` held. Measured: with
//! system GCC 10.3 first on `PATH`, a C++ NuttX image fails with
//! `nuttx_entry_nros_main_generated.cpp:47:1: error: C99 designator
//! 'node_name' outside aggregate initializer` (the designators in
//! `codegen/entry/packs/entry/shared/boot_config.jinja`), and with the store's
//! 13.2-nros5 it builds and runs.
//!
//! [`resolve`](crate::nuttx_toolchain::resolve) is the ONE answer, so the four call sites cannot disagree:
//!
//! - `NUTTX_CROSS` holding a PATH (it has a `/`) is the operator's choice and
//!   is used as given.
//! - `NUTTX_CROSS` holding a bare tool NAME (the riscv board's data sets
//!   `riscv-none-elf-gcc`), or unset (the arm default `arm-none-eabi-gcc`),
//!   names a TOOL. When `nros-sdk-index.toml` pins that tool, the compiler is
//!   CONSTRUCTED as `<store>/sdk/<tool>/<pin>/bin/<tool>` — never found by
//!   listing the store (issue 1546), never taken from `PATH` (issue 0500: a
//!   stale store version on `PATH` shadows the pin). A missing pin REFUSES
//!   with the pin's name and the command that provisions it.
//! - A tool the index does not pin (no checkout above this build: an
//!   out-of-tree consumer) is looked up on `PATH`, with a `cargo:warning`
//!   saying so.
//!
//! The floor is the C++ one `cmake/toolchain/NanoRosCrossToolchain.cmake`
//! enforces on the FreeRTOS road (issue 1936, `NROS_CROSS_GCC_FLOOR_MAJOR`):
//! a C++ image below it is refused before the first compile, by
//! [`CrossGcc::require_cxx_floor`](crate::nuttx_toolchain::CrossGcc::require_cxx_floor). A C-only image keeps building, since the
//! failing constructs are C++.

use std::path::{Path, PathBuf};

/// The C++ floor. Same number as `NROS_CROSS_GCC_FLOOR_MAJOR` in
/// `cmake/toolchain/NanoRosCrossToolchain.cmake`.
pub const GCC_FLOOR_MAJOR: u32 = 11;

/// The arm boards' default tool, when `NUTTX_CROSS` is unset.
pub const DEFAULT_TOOL: &str = "arm-none-eabi-gcc";

/// A resolved cross compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossGcc {
    /// The C driver: an absolute path, or a bare name when it came from `PATH`.
    pub gcc: String,
    /// The C++ driver beside it (`…-gcc` -> `…-g++`).
    pub gxx: String,
    /// Where the answer came from, for messages.
    pub origin: String,
    /// The pin the index names for this tool, if any, for messages.
    pub pin: Option<String>,
    /// The tool name (`arm-none-eabi-gcc`), for messages.
    pub tool: String,
}

impl CrossGcc {
    /// `<prefix>-<suffix>` beside the C driver (`ar`, `objcopy`, …).
    pub fn sibling(&self, suffix: &str) -> String {
        self.gcc
            .strip_suffix("gcc")
            .map(|prefix| format!("{prefix}{suffix}"))
            .unwrap_or_else(|| suffix.to_string())
    }

    /// The driver's major version (`-dumpversion`), `None` when it cannot say.
    pub fn major(&self) -> Option<u32> {
        let out = std::process::Command::new(&self.gcc)
            .arg("-dumpversion")
            .output()
            .ok()?;
        parse_major(&String::from_utf8_lossy(&out.stdout))
    }

    /// Refuse a C++ image on a compiler below [`GCC_FLOOR_MAJOR`]. An
    /// unreadable version is not a refusal: the compile itself will say more.
    pub fn require_cxx_floor(&self) {
        if let Some(major) = self.major()
            && let Some(msg) = below_floor_message(self, major)
        {
            panic!("{msg}");
        }
    }
}

/// Resolve the NuttX cross compiler from `NUTTX_CROSS`, the index pin and the
/// store. Emits the `rerun-if-env-changed` lines it depends on. Panics with the
/// remedy when the pinned tool is not provisioned.
pub fn resolve() -> CrossGcc {
    println!("cargo:rerun-if-env-changed=NUTTX_CROSS");
    println!("cargo:rerun-if-env-changed=NROS_HOME");
    let requested = std::env::var("NUTTX_CROSS").ok().filter(|v| !v.is_empty());
    let tool = requested
        .clone()
        .unwrap_or_else(|| DEFAULT_TOOL.to_string());
    let pin = if tool.contains('/') {
        None
    } else {
        nros_build_paths::sdk_pinned_version(&tool)
    };
    let store_sdk = nros_build_paths::store::root().join("sdk");
    match decide(requested.as_deref(), pin.as_deref(), &store_sdk, |p| {
        p.is_file()
    }) {
        Ok(cross) => {
            if cross.origin == "PATH" {
                println!(
                    "cargo:warning=NuttX cross compiler: `{}` comes from PATH — \
                     nros-sdk-index.toml pins no `{}` here, so no store pin applies",
                    cross.gcc, cross.tool
                );
            }
            // The store file is the compiler: a re-provisioned pin must rebuild.
            if Path::new(&cross.gcc).is_absolute() {
                println!("cargo:rerun-if-changed={}", cross.gcc);
            }
            cross
        }
        Err(msg) => panic!("{msg}"),
    }
}

/// [`resolve`]'s rule over its inputs, so it is testable without a store.
fn decide(
    requested: Option<&str>,
    pin: Option<&str>,
    store_sdk: &Path,
    exists: impl Fn(&Path) -> bool,
) -> Result<CrossGcc, String> {
    let tool = requested.unwrap_or(DEFAULT_TOOL).to_string();
    if tool.contains('/') {
        return Ok(with_gxx(tool.clone(), "$NUTTX_CROSS".into(), None, tool));
    }
    let Some(pin) = pin else {
        return Ok(with_gxx(tool.clone(), "PATH".into(), None, tool));
    };
    let path: PathBuf = store_sdk.join(&tool).join(pin).join("bin").join(&tool);
    if exists(&path) {
        let origin = format!("the SDK store pin {tool} {pin}");
        return Ok(with_gxx(
            path.to_string_lossy().into_owned(),
            origin,
            Some(pin.to_string()),
            tool,
        ));
    }
    Err(format!(
        // phase-548 W3
        "NuttX cross compiler: the pinned {tool} {pin} is not provisioned \
         (expected {}).\n  \
         The build does not fall back to `{tool}` on PATH: that is how a system GCC 10 \
         compiled the image and rejected its C++ designators.\n  \
         Fix:  nros setup --tool {tool}\n  \
         Or:   NUTTX_CROSS=/path/to/bin/{tool}   (any GCC >= {GCC_FLOOR_MAJOR})",
        path.display()
    ))
}

fn with_gxx(gcc: String, origin: String, pin: Option<String>, tool: String) -> CrossGcc {
    let gxx = gcc
        .strip_suffix("gcc")
        .map(|prefix| format!("{prefix}g++"))
        .unwrap_or_else(|| gcc.clone());
    CrossGcc {
        gcc,
        gxx,
        origin,
        pin,
        tool,
    }
}

fn parse_major(dumpversion: &str) -> Option<u32> {
    dumpversion.trim().split('.').next()?.parse().ok()
}

fn below_floor_message(cross: &CrossGcc, major: u32) -> Option<String> {
    if major >= GCC_FLOOR_MAJOR {
        return None;
    }
    let fix = match &cross.pin {
        Some(pin) => format!(
            "nros setup --tool {}      (installs the pinned {pin})",
            cross.tool
        ),
        None => format!("nros setup --tool {}", cross.tool),
    };
    // The missing `std::this_thread`: issue 1936.
    Some(format!(
        "NuttX: a C++ image cannot build here: {} is GCC {major} (via {}), below the C++ \
         floor of GCC {GCC_FLOOR_MAJOR}.\n  \
         GCC 10 rejects the generated entry's designated initializers \
         (`error: C99 designator 'node_name' outside aggregate initializer`), and its newlib \
         libstdc++ declares no `std::this_thread`. Refused before the first \
         compile instead of mid-build.\n  \
         Fix:  {fix}\n  \
         Or:   NUTTX_CROSS=/path/to/bin/{}   (any GCC >= {GCC_FLOOR_MAJOR})",
        cross.gcc, cross.origin, cross.tool
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SDK: &str = "/store/sdk";

    #[test]
    fn unset_resolves_the_arm_pin_in_the_store() {
        let c = decide(None, Some("13.2-nros5"), Path::new(SDK), |_| true).unwrap();
        assert_eq!(
            c.gcc,
            "/store/sdk/arm-none-eabi-gcc/13.2-nros5/bin/arm-none-eabi-gcc"
        );
        assert_eq!(
            c.gxx,
            "/store/sdk/arm-none-eabi-gcc/13.2-nros5/bin/arm-none-eabi-g++"
        );
        assert_eq!(
            c.sibling("ar"),
            "/store/sdk/arm-none-eabi-gcc/13.2-nros5/bin/arm-none-eabi-ar"
        );
    }

    #[test]
    fn a_bare_name_is_a_tool_and_resolves_its_own_pin() {
        let c = decide(
            Some("riscv-none-elf-gcc"),
            Some("14.2-nros1"),
            Path::new(SDK),
            |_| true,
        )
        .unwrap();
        assert_eq!(
            c.gcc,
            "/store/sdk/riscv-none-elf-gcc/14.2-nros1/bin/riscv-none-elf-gcc"
        );
        assert_eq!(
            c.gxx,
            "/store/sdk/riscv-none-elf-gcc/14.2-nros1/bin/riscv-none-elf-g++"
        );
    }

    #[test]
    fn a_missing_pin_refuses_with_its_name_and_never_falls_back_to_path() {
        let e = decide(None, Some("13.2-nros5"), Path::new(SDK), |_| false).unwrap_err();
        assert!(e.contains("arm-none-eabi-gcc 13.2-nros5"), "{e}");
        assert!(e.contains("nros setup --tool arm-none-eabi-gcc"), "{e}");
    }

    #[test]
    fn a_path_is_the_operators_choice_and_is_used_as_given() {
        let c = decide(
            Some("/opt/x/bin/arm-none-eabi-gcc"),
            None,
            Path::new(SDK),
            |_| false,
        )
        .unwrap();
        assert_eq!(c.gcc, "/opt/x/bin/arm-none-eabi-gcc");
        assert_eq!(c.origin, "$NUTTX_CROSS");
    }

    #[test]
    fn an_unpinned_tool_comes_from_path() {
        let c = decide(None, None, Path::new(SDK), |_| false).unwrap();
        assert_eq!(c.gcc, "arm-none-eabi-gcc");
        assert_eq!(c.origin, "PATH");
    }

    #[test]
    fn the_floor_refuses_gcc_10_and_admits_11() {
        let c = decide(None, Some("13.2-nros5"), Path::new(SDK), |_| true).unwrap();
        let m = below_floor_message(&c, 10).expect("GCC 10 is below the floor");
        assert!(m.contains("GCC 10") && m.contains("floor of GCC 11"), "{m}");
        assert!(m.contains("nros setup --tool arm-none-eabi-gcc"), "{m}");
        assert!(below_floor_message(&c, 11).is_none());
        assert!(below_floor_message(&c, 13).is_none());
    }

    #[test]
    fn dumpversion_parses_both_spellings() {
        assert_eq!(parse_major("10.3.1\n"), Some(10));
        assert_eq!(parse_major("13\n"), Some(13));
        assert_eq!(parse_major(""), None);
    }
}
