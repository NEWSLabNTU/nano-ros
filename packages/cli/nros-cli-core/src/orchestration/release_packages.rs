//! phase-527 W6 (RFC-0119 D4, RFC-0097 D8 amendment) — the release underlay.
//!
//! A released toolchain carries nano-ros's own runner and command packages
//! PREBUILT, under `<prefix>/share/nros/packages/<package>/`:
//!
//! ```text
//! share/nros/packages/nros_runner_qemu/
//!   package.xml          copied as is: the announcement the scan reads
//!   nros-runner.toml     the descriptor, with `bin` stated
//!   bin/nros-runner-qemu the executable the release built
//! ```
//!
//! [`stage`] makes that directory from a prefix whose SDK root
//! (`share/nano-ros`, `scripts/stage-sdk-root.sh`) is already staged. The set
//! of packages is not written down anywhere: it is every package in the
//! staged SDK root whose announcements classify as a host tool
//! (`package_xml::kind_role`), found by the one provider scan and resolved by
//! the one descriptor reader, so a new runner or command package ships the day
//! it lands with no edit here or in the workflow.
//!
//! Each staged package's SOURCE copy is removed from the SDK root. Two
//! directories announcing `runner/qemu` in one underlay would make every
//! resolution report the toolchain shadowing itself; a released toolchain
//! ships the package once, prebuilt. `nros-extension` (no announcement) stays.
//!
//! `bin` is stated in the shipped descriptor because nothing could derive it:
//! derivation reads the package's `Cargo.toml`, which a prebuilt package does
//! not carry. The stated value is the derived one, read from the source.

use std::path::{Path, PathBuf};

use cargo_nano_ros::{
    extension_descriptor::{self, ExtensionResolveError},
    package_xml::{BuildRole, COMMAND_KIND, RUNNER_KIND, kind_role},
    provider_scan,
};
use eyre::{Result, WrapErr, bail, eyre};

pub use nros_launcher::checkout::{SHIPPED_PACKAGES_SUBDIR, SHIPPED_SUBDIR};

/// One package [`stage`] wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedPackage {
    /// `<name>` from package.xml.
    pub package: String,
    /// Its host provisions, as `kind:name`.
    pub provides: Vec<String>,
    /// The executable it ships, by name.
    pub bin: String,
    /// Where it was written.
    pub dir: PathBuf,
}

/// Stage every host-tool package in `<prefix>/share/nano-ros` into
/// `<prefix>/share/nros/packages`, taking each executable from
/// `target_dir(<the package's source dir in the checkout>)/release/<bin>`.
///
/// `checkout` is the tree the SDK root was staged from, where the binaries
/// were built; `target_dir` answers a package's cargo target directory (the
/// verb asks `cargo metadata`; a test passes a closure).
///
/// # Errors
///
/// No SDK root under `prefix`; a packages dir already present; a package that
/// does not resolve, or whose executable was not built; any I/O failure.
pub fn stage(
    prefix: &Path,
    checkout: &Path,
    target_dir: &dyn Fn(&Path) -> Result<PathBuf>,
) -> Result<Vec<StagedPackage>> {
    let sdk = prefix.join(SHIPPED_SUBDIR);
    if !sdk.is_dir() {
        bail!(
            "{} is not a staged SDK root — run `scripts/stage-sdk-root.sh {}` first",
            sdk.display(),
            prefix.display()
        );
    }
    let out_root = prefix.join(SHIPPED_PACKAGES_SUBDIR);
    if out_root.exists() {
        bail!(
            "{} already exists — stage into a fresh prefix",
            out_root.display()
        );
    }
    let scan = provider_scan::scan_roots(std::slice::from_ref(&sdk))
        .map_err(|e| eyre!("scanning {}: {e}", sdk.display()))?;
    // A malformed package.xml elsewhere in the SDK root is the scan's to
    // report, not this stage's to refuse: it cannot be a host tool this
    // stage would miss silently, because `check-provider-announcements`
    // refuses an unreadable announcement in the tree it was staged from.
    for e in &scan.errors {
        eprintln!("note: {}: {}", e.path.display(), e.message);
    }

    let mut staged = Vec::new();
    for pkg in &scan.providers {
        let host: Vec<_> = pkg
            .provides
            .iter()
            .filter(|p| kind_role(&p.kind) == Some(BuildRole::Host))
            .collect();
        if host.is_empty() {
            continue;
        }
        let rel = pkg
            .dir
            .strip_prefix(&sdk)
            .wrap_err("a scanned package lies outside the root it was scanned under")?;
        let source = checkout.join(rel);
        let dest = out_root.join(&pkg.package);
        if dest.exists() {
            bail!(
                "two packages named `{}` in {} — package names are unique",
                pkg.package,
                sdk.display()
            );
        }
        std::fs::create_dir_all(dest.join("bin"))?;
        std::fs::copy(pkg.dir.join("package.xml"), dest.join("package.xml"))
            .wrap_err_with(|| format!("copying {}/package.xml", pkg.dir.display()))?;

        let mut bins = Vec::new();
        for p in &host {
            let bin = resolved_bin(&scan, &p.kind, &p.name)
                .map_err(|e| eyre!("`{}` ({}:{}): {e}", pkg.package, p.kind, p.name))?;
            let descriptor = pkg.descriptor_path(&p.kind);
            let file = descriptor
                .file_name()
                .expect("a descriptor path has a file name");
            let text = std::fs::read_to_string(&descriptor)?;
            std::fs::write(dest.join(file), with_bin_stated(&text, &bin))?;
            if !bins.contains(&bin) {
                bins.push(bin);
            }
        }
        let target = target_dir(&source)?;
        for bin in &bins {
            let built = target.join("release").join(bin);
            if !built.is_file() {
                bail!(
                    "`{}`'s executable {} is not built — build it first \
                     (`cargo build --release --locked --manifest-path {}/Cargo.toml`)",
                    pkg.package,
                    built.display(),
                    source.display()
                );
            }
            let shipped = dest.join("bin").join(bin);
            std::fs::copy(&built, &shipped)
                .wrap_err_with(|| format!("copying {}", built.display()))?;
            set_executable(&shipped)?;
        }
        std::fs::remove_dir_all(&pkg.dir)
            .wrap_err_with(|| format!("removing the source copy {}", pkg.dir.display()))?;
        staged.push(StagedPackage {
            package: pkg.package.clone(),
            provides: host
                .iter()
                .map(|p| format!("{}:{}", p.kind, p.name))
                .collect(),
            bin: bins.join(", "),
            dir: dest,
        });
    }

    verify(&out_root, &staged)?;
    Ok(staged)
}

/// The executable a host provision resolves to, through the one descriptor
/// reader (which validates the descriptor and derives `bin`).
fn resolved_bin(
    scan: &provider_scan::ScanResult,
    kind: &str,
    name: &str,
) -> Result<String, ExtensionResolveError> {
    match kind {
        RUNNER_KIND => extension_descriptor::resolve_runner_in(scan, name).map(|r| r.bin),
        COMMAND_KIND => extension_descriptor::resolve_command_in(scan, name).map(|c| c.bin),
        other => unreachable!("`{other}` is not a host kind this stage knows"),
    }
}

/// The descriptor text with `bin` stated: unchanged when it already states
/// one (the resolution above checked it), otherwise one line appended.
fn with_bin_stated(text: &str, bin: &str) -> String {
    let states_bin = text
        .lines()
        .any(|l| l.trim_start().starts_with("bin") && l.contains('='));
    if states_bin {
        return text.to_string();
    }
    let mut out = text.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&format!(
        "# Stated by the release (phase-527 W6): a prebuilt package ships no\n\
         # Cargo.toml to derive it from. It is the bin target the source derives.\n\
         bin = \"{bin}\"\n"
    ));
    out
}

#[cfg(unix)]
fn set_executable(p: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_: &Path) -> Result<()> {
    Ok(())
}

/// The staged directory resolves the way a released `nros` will read it:
/// every provision through the same reader, to a `bin/` that exists.
fn verify(out_root: &Path, staged: &[StagedPackage]) -> Result<()> {
    if staged.is_empty() {
        return Ok(());
    }
    let scan = provider_scan::scan_roots(&[out_root.to_path_buf()])
        .map_err(|e| eyre!("re-scanning {}: {e}", out_root.display()))?;
    for s in staged {
        for prov in &s.provides {
            let (kind, name) = prov.split_once(':').expect("kind:name");
            let bin = resolved_bin(&scan, kind, name)
                .map_err(|e| eyre!("the staged `{}` does not resolve: {e}", s.package))?;
            if !s.dir.join("bin").join(&bin).is_file() {
                bail!("the staged `{}` resolves to a missing bin/{bin}", s.package);
            }
        }
    }
    Ok(())
}

/// A package's cargo target directory, as `cargo metadata` resolves it (so a
/// `CARGO_TARGET_DIR` or `[build] target-dir` is honoured as the build
/// honoured it).
///
/// # Errors
///
/// `cargo metadata` failing, or reporting no target directory.
pub fn cargo_target_dir(package_dir: &Path) -> Result<PathBuf> {
    let out =
        std::process::Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .args(["metadata", "--format-version=1", "--no-deps"])
            .arg("--manifest-path")
            .arg(package_dir.join("Cargo.toml"))
            .output()
            .wrap_err("running `cargo metadata`")?;
    if !out.status.success() {
        bail!(
            "`cargo metadata` failed in {}: {} (a staged host-tool package must be a \
             cargo package; a CMake one is not supported yet)",
            package_dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    meta.get("target_directory")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .ok_or_else(|| eyre!("`cargo metadata` reported no target_directory"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    fn host_pkg(root: &Path, rel: &str, name: &str, kind: &str, provides: &str, bin: &str) {
        let dir = root.join(rel);
        write(
            &dir.join("package.xml"),
            &format!(
                "<?xml version=\"1.0\"?>\n<package format=\"3\"><name>{name}</name>\
                 <version>0.0.0</version><description>d</description>\
                 <maintainer email=\"a@b.c\">a</maintainer><license>MIT</license>\
                 <export><build_type>nros_cargo</build_type>\
                 <nano_ros_provides kind=\"{kind}\" name=\"{provides}\"/></export></package>\n"
            ),
        );
        write(
            &dir.join("Cargo.toml"),
            &format!(
                "[package]\nname = \"{name}\"\nversion = \"0.0.0\"\n\n[[bin]]\nname = \"{bin}\"\npath = \"src/main.rs\"\n"
            ),
        );
        write(&dir.join("src/main.rs"), "fn main() {}\n");
        let descriptor = if kind == "runner" {
            "protocol = 1\nverbs = [\"run\"]\n"
        } else {
            "protocol = 1\nsummary = \"a verb\"\n"
        };
        write(&dir.join(format!("nros-{kind}.toml")), descriptor);
    }

    /// A staged SDK root holding one runner, one command and one plain
    /// package; the checkout beside it holds the built executables.
    fn fixture(tmp: &Path) -> (PathBuf, PathBuf) {
        let prefix = tmp.join("prefix");
        let sdk = prefix.join(SHIPPED_SUBDIR);
        let checkout = tmp.join("checkout");
        for root in [&sdk, &checkout] {
            host_pkg(
                root,
                "packages/runners/acme_runner",
                "acme_runner",
                "runner",
                "acme",
                "nros-runner-acme",
            );
            host_pkg(
                root,
                "packages/rmw/x/acme_tools",
                "acme_tools",
                "command",
                "acme",
                "nros-acme-tools",
            );
            write(
                &root.join("packages/runners/nros-extension/Cargo.toml"),
                "[package]\nname = \"nros-extension\"\n",
            );
        }
        for bin in ["nros-runner-acme", "nros-acme-tools"] {
            write(&checkout.join("target/release").join(bin), "#!/bin/sh\n");
        }
        (prefix, checkout)
    }

    #[test]
    fn every_host_package_is_staged_prebuilt_and_its_source_leaves_the_sdk_root() {
        let tmp = tempfile::tempdir().unwrap();
        let (prefix, checkout) = fixture(tmp.path());
        let target = checkout.join("target");
        let staged = stage(&prefix, &checkout, &|_| Ok(target.clone())).unwrap();
        let names: Vec<_> = staged.iter().map(|s| s.package.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"acme_runner") && names.contains(&"acme_tools"));

        let out = prefix.join(SHIPPED_PACKAGES_SUBDIR);
        let runner = out.join("acme_runner");
        assert!(runner.join("package.xml").is_file());
        assert!(runner.join("bin/nros-runner-acme").is_file());
        let d = std::fs::read_to_string(runner.join("nros-runner.toml")).unwrap();
        assert!(d.contains("bin = \"nros-runner-acme\""), "{d}");
        assert!(
            !runner.join("Cargo.toml").exists(),
            "a prebuilt package ships no source"
        );
        assert!(out.join("acme_tools/bin/nros-acme-tools").is_file());

        let sdk = prefix.join(SHIPPED_SUBDIR);
        assert!(!sdk.join("packages/runners/acme_runner").exists());
        assert!(!sdk.join("packages/rmw/x/acme_tools").exists());
        assert!(
            sdk.join("packages/runners/nros-extension/Cargo.toml")
                .is_file(),
            "a package announcing nothing stays in the SDK root"
        );

        // The released search path resolves the runner to the prebuilt copy.
        let ws = tmp.path().join("ws");
        std::fs::create_dir_all(&ws).unwrap();
        let path = provider_scan::build_search_path(Some(&sdk), &ws, &[], None);
        let scan = provider_scan::scan_search_path(&path).unwrap();
        let r = extension_descriptor::resolve_runner_in(&scan, "acme").unwrap();
        assert_eq!(r.dir, runner.canonicalize().unwrap_or(runner.clone()));
        assert_eq!(r.bin, "nros-runner-acme");
    }

    #[test]
    fn an_unbuilt_executable_is_refused_naming_the_build() {
        let tmp = tempfile::tempdir().unwrap();
        let (prefix, checkout) = fixture(tmp.path());
        std::fs::remove_file(checkout.join("target/release/nros-runner-acme")).unwrap();
        let target = checkout.join("target");
        let e = stage(&prefix, &checkout, &|_| Ok(target.clone())).unwrap_err();
        assert!(format!("{e:#}").contains("is not built"), "{e:#}");
    }

    #[test]
    fn a_stated_bin_is_kept_and_an_unstated_one_is_appended() {
        assert_eq!(
            with_bin_stated("protocol = 1\nbin = \"x\"\n", "x"),
            "protocol = 1\nbin = \"x\"\n"
        );
        let t = with_bin_stated("protocol = 1", "y");
        assert!(
            t.starts_with("protocol = 1\n") && t.ends_with("bin = \"y\"\n"),
            "{t}"
        );
    }
}
