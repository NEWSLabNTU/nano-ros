use std::{
    collections::HashMap,
    env,
    path::{Path, PathBuf},
};

pub type SizeMap = HashMap<String, u64>;

const NUTTX_FALLBACK_SIZES: &[(&str, u64)] = &[
    ("EXECUTOR_SIZE", 79_296),
    ("GUARD_CONDITION_SIZE", 24),
    ("PUBLISHER_SIZE", 560),
    ("SUBSCRIBER_SIZE", 560),
    ("SERVICE_CLIENT_SIZE", 4_632),
    ("SERVICE_SERVER_SIZE", 528),
    ("SESSION_SIZE", 528),
    ("LIFECYCLE_CTX_SIZE", 64),
    ("ACTION_SERVER_INTERNAL_SIZE", 96),
    ("ACTION_SERVER_RAW_HANDLE_SIZE", 48),
    ("RAW_SUBSCRIPTION_SIZE", 205 * 8),
    ("RAW_SERVICE_SERVER_SIZE", 194 * 8),
    ("RAW_SERVICE_CLIENT_SIZE", 707 * 8),
    ("RAW_ACTION_SERVER_SIZE", 786 * 8),
    ("RAW_ACTION_CLIENT_SIZE", 2_193 * 8),
];

pub fn probe_nros_sizes(crate_label: &str) -> SizeMap {
    let rlib = match nros_sizes_build::find_dep_rlib("nros", "__NROS_SIZE_") {
        Ok(p) => p,
        Err(e) => {
            println!("cargo:warning={crate_label} probe: {e}");
            let mut map = HashMap::new();
            apply_nuttx_size_fallbacks(crate_label, &mut map);
            return map;
        }
    };
    let mut map = match nros_sizes_build::extract_sizes(&rlib, "__NROS_SIZE_") {
        Ok(map) => map,
        Err(e) => {
            println!(
                "cargo:warning={crate_label} probe failed parsing {}: {e}",
                rlib.display()
            );
            HashMap::new()
        }
    };
    apply_nuttx_size_fallbacks(crate_label, &mut map);
    map
}

fn apply_nuttx_size_fallbacks(crate_label: &str, map: &mut SizeMap) {
    let target = env::var("TARGET").unwrap_or_default();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "nuttx" && !target.contains("nuttx") {
        return;
    }
    if map.get("EXECUTOR_SIZE").copied().unwrap_or(0) != 0 {
        return;
    }
    println!(
        "cargo:warning={crate_label} probe returned no NuttX sizes; using committed NuttX fallback sizes"
    );
    for (name, value) in NUTTX_FALLBACK_SIZES {
        map.insert((*name).to_string(), *value);
    }
}

pub fn compile_c_stub(
    manifest_dir: &Path,
    rel_path: &str,
    include_dir: Option<&Path>,
    lib_name: &str,
    use_baremetal_libc: bool,
) {
    let path = manifest_dir.join(rel_path);
    println!("cargo:rerun-if-changed={}", path.display());
    let mut build = cc::Build::new();
    build
        .file(&path)
        .warnings(true)
        .extra_warnings(true)
        .flag_if_supported("-Wpedantic");
    if let Some(include_dir) = include_dir {
        build.include(include_dir);
    }
    if use_baremetal_libc {
        apply_baremetal_libc(&mut build);
    }
    build.compile(lib_name);
}

pub fn apply_baremetal_libc(build: &mut cc::Build) {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if arch != "riscv64" || os != "none" {
        return;
    }
    println!("cargo:rerun-if-env-changed=NROS_PICOLIBC_SYSROOT");
    if let Some(include) = picolibc_include() {
        build.flag("-isystem").flag(&include);
    }
}

pub fn picolibc_include() -> Option<String> {
    if let Ok(root) = env::var("NROS_PICOLIBC_SYSROOT") {
        let include = format!("{root}/include");
        if Path::new(&include).is_dir() {
            return Some(include);
        }
    }
    if let Ok(output) = std::process::Command::new("riscv64-unknown-elf-gcc")
        .args([
            "-march=rv64gc",
            "-mabi=lp64d",
            "--specs=picolibc.specs",
            "-print-sysroot",
        ])
        .output()
    {
        let sysroot = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !sysroot.is_empty() {
            let include = format!("{sysroot}/include");
            if Path::new(&include).is_dir() {
                return Some(include);
            }
        }
    }
    let fallback = "/usr/lib/picolibc/riscv64-unknown-elf/include";
    if Path::new(fallback).is_dir() {
        return Some(fallback.to_string());
    }
    None
}

pub fn generate_cbindgen_header(manifest_dir: &Path, config_name: &str, output_rel: &str) {
    let config_path = manifest_dir.join(config_name);
    let output_path = manifest_dir.join(output_rel);

    let config = match cbindgen::Config::from_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            println!("cargo:warning=Failed to load cbindgen config: {e}");
            return;
        }
    };

    let result = cbindgen::Builder::new()
        .with_crate(manifest_dir)
        .with_config(config)
        .generate();

    match result {
        Ok(bindings) => write_cbindgen_header_serialized(&output_path, bindings),
        Err(e) => {
            println!("cargo:warning=cbindgen header generation skipped: {e}");
        }
    }
}

/// Serialize concurrent regenerators of this shared source-tree header.
///
/// The cbindgen headers (`nros_generated.h` / `nros_cpp_ffi.h`) are committed
/// to the source tree and regenerated **in place** by every parallel `build.rs`
/// invocation. On a cold workspace build, N independent Corrosion / Cargo build
/// trees (e.g. the threadx-linux C++ fixtures, which — unlike nuttx — are not
/// serialized with `NROS_CARGO_FRONTENDS=1`) run this code concurrently against
/// the *same* output path. The inner `write_cbindgen_header_atomically` makes a
/// single writer's replacement atomic, but nothing serializes N writers racing
/// on the write/compare/rename sequence, so a concurrent reader (a C++ compile
/// `#include`-ing the header) could observe an intermediate state across the
/// burst (known-issues #15: transient "multiple definition / conflicting
/// declaration of `nros_cpp_qos_t`").
///
/// A cross-process advisory lock keyed on the *absolute output path* (so it is
/// shared across distinct target dirs that all write the one source-tree file)
/// makes the whole "generate fresh contents → atomically replace" critical
/// section mutually exclusive. The lockfile lives in the host temp dir, so it
/// adds no source-tree / git noise. On non-unix hosts the lock is a no-op and we
/// fall back to the atomic rename alone.
fn write_cbindgen_header_serialized(output_path: &Path, bindings: cbindgen::Bindings) {
    let _guard = HeaderLock::acquire(output_path);
    write_cbindgen_header_atomically(output_path, bindings);
}

/// Cross-process advisory lock guarding regeneration of one shared header.
///
/// Holds the open lockfile for the guard's lifetime; the kernel releases the
/// `flock` when the descriptor is closed on drop.
struct HeaderLock {
    #[cfg(unix)]
    _file: Option<std::fs::File>,
}

impl HeaderLock {
    fn acquire(output_path: &Path) -> Self {
        #[cfg(unix)]
        {
            use std::{
                collections::hash_map::DefaultHasher,
                hash::{Hash, Hasher},
                os::unix::io::AsRawFd,
            };

            // Key the lock on the absolute output path so all concurrent
            // regenerators of the same header agree on one lockfile,
            // regardless of their (differing) cargo target dirs.
            let abs =
                std::fs::canonicalize(output_path).unwrap_or_else(|_| output_path.to_path_buf());
            let mut hasher = DefaultHasher::new();
            abs.hash(&mut hasher);
            let lock_path = env::temp_dir().join(format!(
                "nros-cbindgen-header-{:016x}.lock",
                hasher.finish()
            ));

            let file = match std::fs::OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(&lock_path)
            {
                Ok(f) => f,
                // If the lockfile can't be opened, fall back to the bare
                // atomic rename (still safe against partial reads).
                Err(_) => return HeaderLock { _file: None },
            };

            // Blocking exclusive advisory lock, retrying on EINTR. Released
            // automatically when `file` is dropped (descriptor close).
            unsafe extern "C" {
                fn flock(fd: i32, op: i32) -> i32;
            }
            const LOCK_EX: i32 = 2;
            const EINTR: i32 = 4;
            loop {
                let rc = unsafe { flock(file.as_raw_fd(), LOCK_EX) };
                if rc == 0 {
                    break;
                }
                if std::io::Error::last_os_error().raw_os_error() != Some(EINTR) {
                    // Lock failed for a non-recoverable reason; proceed with
                    // the atomic rename alone rather than blocking the build.
                    return HeaderLock { _file: None };
                }
            }
            HeaderLock { _file: Some(file) }
        }
        #[cfg(not(unix))]
        {
            let _ = output_path;
            HeaderLock {}
        }
    }

    /// Whether a real exclusive advisory lock is held (false when the lock
    /// degraded to a no-op). Used only by the serialization unit test.
    #[cfg(all(test, unix))]
    fn is_real(&self) -> bool {
        self._file.is_some()
    }
}

fn write_cbindgen_header_atomically(output_path: &Path, bindings: cbindgen::Bindings) {
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let tmp = output_path.with_file_name(format!(
        ".{}.tmp.{}",
        output_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("cbindgen-header"),
        std::process::id()
    ));
    bindings.write_to_file(&tmp);
    let differs = std::fs::read(&tmp).ok() != std::fs::read(output_path).ok();
    if differs {
        std::fs::rename(&tmp, output_path).ok();
    } else {
        std::fs::remove_file(&tmp).ok();
    }
}

pub fn write_header_to_target_dir(relative: &[&str], contents: &str) {
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        write_to(PathBuf::from(target_dir), relative, contents);
    } else if let Ok(target_dir) = nros_sizes_build::cargo_target_dir() {
        write_to(target_dir, relative, contents);
    }
}

fn write_to(root: PathBuf, relative: &[&str], contents: &str) {
    let mut dest = root;
    for segment in relative {
        dest.push(segment);
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).expect("create per-build header dir");
    }
    std::fs::write(&dest, contents).expect("write per-build header");
}

pub fn write_header_to_corrosion(filename: &str, contents: &str) {
    let Ok(corrosion_dir) = env::var("CORROSION_BUILD_DIR") else {
        return;
    };
    let dest = PathBuf::from(corrosion_dir).join(filename);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).expect("create corrosion header dir");
    }
    std::fs::write(&dest, contents).expect("write corrosion header");
}

pub fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

pub fn dep_usize(name: &str) -> usize {
    env::var(name)
        .unwrap_or_else(|_| {
            panic!("{name} not set — is nros-node's `links = \"nros_node\"` configured?")
        })
        .parse()
        .unwrap_or_else(|_| panic!("{name} is not a valid usize"))
}

pub fn target_pointer_bytes() -> usize {
    match env::var("CARGO_CFG_TARGET_POINTER_WIDTH").ok().as_deref() {
        Some("32") => 4,
        Some("64") => 8,
        _ => core::mem::size_of::<*const ()>(),
    }
}

pub fn non_zero_or(probe: usize, fallback: usize) -> usize {
    if probe != 0 { probe } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_zero_or_prefers_probe() {
        assert_eq!(non_zero_or(24, 48), 24);
        assert_eq!(non_zero_or(0, 48), 48);
    }

    // Proves the cross-process advisory lock guarding cbindgen header
    // regeneration (known-issues #15) actually serializes the critical
    // section: many concurrent holders of `HeaderLock` keyed on the same
    // output path must never overlap. flock locks are associated with the
    // open file description, so independent `open()`s — even within one
    // process — mutually exclude, which is exactly what concurrent `build.rs`
    // invocations do.
    #[cfg(unix)]
    #[test]
    fn header_lock_serializes_concurrent_holders() {
        use std::{
            sync::{
                Arc,
                atomic::{AtomicUsize, Ordering},
            },
            thread,
        };

        // A unique-per-run synthetic output path; canonicalize() will fail and
        // fall back to the raw path, so all threads hash the same key.
        let key = env::temp_dir().join(format!(
            "nros-header-lock-test-{}-{}.h",
            std::process::id(),
            "qos"
        ));

        let in_section = Arc::new(AtomicUsize::new(0));
        let max_overlap = Arc::new(AtomicUsize::new(0));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let key = key.clone();
                let in_section = Arc::clone(&in_section);
                let max_overlap = Arc::clone(&max_overlap);
                thread::spawn(move || {
                    for _ in 0..200 {
                        let _guard = HeaderLock::acquire(&key);
                        // Skip the assertion entirely if the lock degraded to a
                        // no-op (e.g. temp dir unwritable) — we only assert the
                        // mutual-exclusion property when a real lock was taken.
                        if !_guard.is_real() {
                            continue;
                        }
                        let now = in_section.fetch_add(1, Ordering::SeqCst) + 1;
                        max_overlap.fetch_max(now, Ordering::SeqCst);
                        // Encourage interleaving if the lock failed to exclude.
                        thread::yield_now();
                        in_section.fetch_sub(1, Ordering::SeqCst);
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // If any real lock was taken, the section must never have had >1
        // concurrent holder.
        assert!(
            max_overlap.load(Ordering::SeqCst) <= 1,
            "HeaderLock allowed {} concurrent holders — serialization broken",
            max_overlap.load(Ordering::SeqCst)
        );

        let _ = std::fs::remove_file(&key);
    }
}
