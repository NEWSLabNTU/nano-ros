//! Discover codegen inputs: `package.xml` + every `.msg` / `.srv` / `.action`
//! file under the package root, plus interface-package build_depend names.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct Discovery {
    /// Absolute path to `package.xml`.
    pub package_xml: PathBuf,
    /// Discovered `.msg` / `.srv` / `.action` files (sorted).
    pub interface_files: Vec<PathBuf>,
    /// Names parsed from `<build_depend>` / `<depend>` tags.
    pub build_depends: Vec<String>,
}

/// Walk the directory containing `package.xml` and collect codegen inputs.
pub fn discover(package_xml: &Path) -> io::Result<Discovery> {
    let pkg_root = package_xml.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "package.xml has no parent directory",
        )
    })?;

    let mut interface_files = Vec::new();
    for entry in WalkDir::new(pkg_root).follow_links(false) {
        let entry = entry.map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let p = entry.path();
        if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
            if matches!(ext, "msg" | "srv" | "action") {
                interface_files.push(p.to_path_buf());
            }
        }
    }
    interface_files.sort();

    let xml = fs::read_to_string(package_xml)?;
    let build_depends = parse_build_depends(&xml);

    Ok(Discovery {
        package_xml: package_xml.to_path_buf(),
        interface_files,
        build_depends,
    })
}

/// Tiny tag scanner — extracts `<build_depend>NAME</build_depend>` and
/// `<depend>NAME</depend>` payloads. No full XML parser dependency.
fn parse_build_depends(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    for tag in ["build_depend", "depend"] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        let mut cursor = 0usize;
        while let Some(start) = xml[cursor..].find(&open) {
            let s = cursor + start + open.len();
            if let Some(end_rel) = xml[s..].find(&close) {
                let name = xml[s..s + end_rel].trim().to_string();
                if !name.is_empty() && !out.contains(&name) {
                    out.push(name);
                }
                cursor = s + end_rel + close.len();
            } else {
                break;
            }
        }
    }
    out
}
