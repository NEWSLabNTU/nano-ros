//! Which `portable/` directory a FreeRTOS port name means — phase-349 W3.
//!
//! `FREERTOS_PORT` is upstream FreeRTOS-Kernel's port ENUM (`GCC_ARM_CM3`),
//! the vocabulary upstream's own `CMakeLists.txt` takes. The pairing between
//! an enum and its directory under `portable/` is READ from upstream's
//! `portable/CMakeLists.txt` header-directory table:
//!
//! ```text
//! $<$<STREQUAL:${FREERTOS_PORT},GCC_ARM_CM3>:${CMAKE_CURRENT_LIST_DIR}/GCC/ARM_CM3>
//! ```
//!
//! never restated here. The path is not the enum with `/` for `_`:
//! `GCC/ARM_CRx_No_GIC` is `GCC_ARM_CRX_NOGIC` and `ThirdParty/GCC/Posix` is
//! `GCC_POSIX`, so a hand table would be one more copy of a fact upstream owns.
//! The CMake twin is `_nros_freertos_port_table` in
//! `packages/api/nros-c/cmake/nros-freertos.cmake`; both parse the same rows.
//!
//! A path under `portable/` is still accepted, in two cases that differ:
//!
//! * a path upstream LISTS (`GCC/ARM_CM3`) is the deprecated spelling of an
//!   enum. [`PortDir::deprecated_alias_of`] names the enum so the caller can
//!   say so; the spelling is retired one release after phase-349 W3.
//! * a path upstream does NOT list (NXP's `GCC/ARM_CR52_GIC`, or any port in
//!   a kernel tree that predates upstream's CMake) has no enum at all, so the
//!   path is its only spelling and nothing is deprecated. It must hold a
//!   `port.c`.
//!
//! [`PortDir::deprecated_alias_of`]: crate::freertos_port::PortDir::deprecated_alias_of

use std::path::{Path, PathBuf};

/// One resolved port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortDir {
    /// `<freertos_dir>/portable/<path>`.
    pub dir: PathBuf,
    /// The path under `portable/`.
    pub rel: String,
    /// Set when the caller wrote a listed PATH: the enum it stands for.
    pub deprecated_alias_of: Option<String>,
}

/// Upstream's `(enum, path)` rows, in table order. Empty when the tree has no
/// `portable/CMakeLists.txt` (a kernel older than V10.5.0, or a vendor tree).
pub fn port_table(freertos_dir: &Path) -> Vec<(String, String)> {
    std::fs::read_to_string(freertos_dir.join("portable/CMakeLists.txt"))
        .map(|text| parse_port_table(&text))
        .unwrap_or_default()
}

/// The header-directory rows of upstream's `portable/CMakeLists.txt`. A row
/// may wrap after the `:` (`GCC_POSIX>:` then the directory on the next
/// line), so whitespace there is skipped. Source-list rows name a `.c` file
/// with no `${CMAKE_CURRENT_LIST_DIR}/` prefix and are not matched.
pub fn parse_port_table(text: &str) -> Vec<(String, String)> {
    const OPEN: &str = "STREQUAL:${FREERTOS_PORT},";
    const DIR: &str = "${CMAKE_CURRENT_LIST_DIR}/";
    let mut rows = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find(OPEN) {
        rest = &rest[at + OPEN.len()..];
        let Some(close) = rest.find(">:") else { break };
        let name = &rest[..close];
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let after = rest[close + 2..].trim_start();
        let Some(path) = after.strip_prefix(DIR) else {
            continue;
        };
        let end = path
            .find(|c: char| c == '>' || c.is_whitespace())
            .unwrap_or(path.len());
        if end > 0 {
            rows.push((name.to_string(), path[..end].to_string()));
        }
    }
    rows
}

/// Resolve `port` (an enum, or a path under `portable/`) against the kernel
/// at `freertos_dir`.
pub fn resolve(freertos_dir: &Path, port: &str) -> Result<PortDir, String> {
    resolve_in(freertos_dir, port, &port_table(freertos_dir))
}

fn resolve_in(
    freertos_dir: &Path,
    port: &str,
    table: &[(String, String)],
) -> Result<PortDir, String> {
    let port = port.trim();
    let portable = freertos_dir.join("portable");
    if port.contains('/') {
        if portable.join(port).join("port.c").is_file() {
            let alias = table
                .iter()
                .find(|(_, p)| p == port)
                .map(|(e, _)| e.clone());
            return Ok(PortDir {
                dir: portable.join(port),
                rel: port.to_string(),
                deprecated_alias_of: alias,
            });
        }
        return Err(format!(
            "FREERTOS_PORT `{port}` is a path, and {} holds no port.c",
            portable.join(port).display()
        ));
    }
    if let Some((_, rel)) = table.iter().find(|(e, _)| e == port) {
        return Ok(PortDir {
            dir: portable.join(rel),
            rel: rel.clone(),
            deprecated_alias_of: None,
        });
    }
    let source = portable.join("CMakeLists.txt");
    if table.is_empty() {
        return Err(format!(
            "FREERTOS_PORT `{port}` is an upstream port name, but {} has no port table \
             ({} is missing or lists nothing) — that kernel predates upstream's CMake, so \
             spell the port as its path under portable/ (e.g. GCC/ARM_CM3)",
            freertos_dir.display(),
            source.display()
        ));
    }
    Err(format!(
        "FREERTOS_PORT `{port}` is not a port {} lists. Upstream's names are the \
         `GCC_ARM_CM3` style; a vendor port is spelled as its path under portable/",
        source.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "\
add_library(freertos_kernel_port OBJECT
    $<$<STREQUAL:${FREERTOS_PORT},GCC_ARM_CM3>:
        GCC/ARM_CM3/port.c>
)
target_include_directories(freertos_kernel_port_headers INTERFACE
    $<$<STREQUAL:${FREERTOS_PORT},GCC_ARM_CM3>:${CMAKE_CURRENT_LIST_DIR}/GCC/ARM_CM3>
    $<$<STREQUAL:${FREERTOS_PORT},GCC_ARM_CM7>:${CMAKE_CURRENT_LIST_DIR}/GCC/ARM_CM7/r0p1>
    $<$<STREQUAL:${FREERTOS_PORT},GCC_ARM_CRX_NOGIC>:${CMAKE_CURRENT_LIST_DIR}/GCC/ARM_CRx_No_GIC>
    $<$<STREQUAL:${FREERTOS_PORT},GCC_POSIX>:
        ${CMAKE_CURRENT_LIST_DIR}/ThirdParty/GCC/Posix
        ${CMAKE_CURRENT_LIST_DIR}/ThirdParty/GCC/Posix/utils>
)
";

    #[test]
    fn header_rows_parse_and_source_rows_do_not() {
        let rows = parse_port_table(TABLE);
        let names: Vec<_> = rows.iter().map(|(e, p)| format!("{e}={p}")).collect();
        assert_eq!(
            names,
            [
                "GCC_ARM_CM3=GCC/ARM_CM3",
                "GCC_ARM_CM7=GCC/ARM_CM7/r0p1",
                "GCC_ARM_CRX_NOGIC=GCC/ARM_CRx_No_GIC",
                "GCC_POSIX=ThirdParty/GCC/Posix",
            ]
        );
    }

    fn kernel(tag: &str) -> (PathBuf, Vec<(String, String)>) {
        let dir = std::env::temp_dir().join(format!("nros-port-{tag}-{}", std::process::id()));
        for p in ["GCC/ARM_CM3", "GCC/ARM_CR52_GIC"] {
            let d = dir.join("portable").join(p);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("port.c"), "").unwrap();
        }
        (dir, parse_port_table(TABLE))
    }

    #[test]
    fn an_enum_resolves_to_its_directory() {
        let (k, t) = kernel("enum");
        let r = resolve_in(k.as_path(), "GCC_ARM_CM3", &t).unwrap();
        assert_eq!(r.dir, k.as_path().join("portable/GCC/ARM_CM3"));
        assert_eq!(r.deprecated_alias_of, None);
    }

    #[test]
    fn a_listed_path_is_a_deprecated_alias_naming_its_enum() {
        let (k, t) = kernel("alias");
        let r = resolve_in(k.as_path(), "GCC/ARM_CM3", &t).unwrap();
        assert_eq!(r.rel, "GCC/ARM_CM3");
        assert_eq!(r.deprecated_alias_of.as_deref(), Some("GCC_ARM_CM3"));
    }

    #[test]
    fn an_unlisted_vendor_path_is_its_own_spelling() {
        let (k, t) = kernel("vendor");
        let r = resolve_in(k.as_path(), "GCC/ARM_CR52_GIC", &t).unwrap();
        assert_eq!(r.deprecated_alias_of, None);
    }

    #[test]
    fn unknown_names_and_empty_paths_are_refused() {
        let (k, t) = kernel("refused");
        let e = resolve_in(k.as_path(), "GCC_ARM_CM99", &t).unwrap_err();
        assert!(
            e.contains("GCC_ARM_CM99") && e.contains("CMakeLists.txt"),
            "{e}"
        );
        let e = resolve_in(k.as_path(), "GCC/NOPE", &t).unwrap_err();
        assert!(e.contains("port.c"), "{e}");
        let e = resolve_in(k.as_path(), "GCC_ARM_CM3", &[]).unwrap_err();
        assert!(e.contains("no port table"), "{e}");
    }
}
