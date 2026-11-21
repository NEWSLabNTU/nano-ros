//! Does a built image link an allocator? The reader behind `nros image check
//! --no-alloc` (issue 2667), and the one home of the RTOS kernel-allocator
//! naming scheme (issue 1616).
//!
//! # What counts as an allocation symbol
//!
//! Four families, each a different way the heap arrives, reported separately:
//!
//! * `c-heap`: `malloc` / `calloc` / `realloc` / `free`, their aligned and
//!   duplicating variants (`aligned_alloc`, `strdup`, …), and the libc
//!   spellings that alias them (`__libc_malloc`, newlib's `_malloc_r`). The
//!   duplicating ones are here because `strdup` allocates.
//! * `rust-alloc-shim`: `__rust_alloc` and its siblings, the `__rg_*` symbols
//!   a `#[global_allocator]` expands to, the `__rdl_*` default shim, and
//!   `__rust_no_alloc_shim_is_unstable`. Present if and only if the `alloc`
//!   crate is linked, whatever the feature flags say. The one family matched
//!   inside a name: rustc emits it in a synthetic `__rustc` crate, so a real
//!   image spells it as a v0-mangled `__rustc` path ending in `__rust_alloc`.
//! * `cxx-operator-new`: `operator new` / `operator delete` and their array,
//!   sized and aligned overloads, by Itanium mangling (`_Znw`, `_Zna`,
//!   `_Zdl`, `_Zda`).
//! * `rtos-heap`: each kernel's allocator API, matched by its naming scheme
//!   ([`KERNEL_ALLOC_SCHEME`]), never by a list of names.
//!
//! Everything but the Rust family matches a WHOLE name, or the last segment of
//! a mangled path (`std::free`): `freeaddrinfo` is a real symbol in this
//! tree's images and allocates nothing, and a check that fires on it gets a
//! blanket exemption on its first run.
//!
//! # Tiers
//!
//! `heap-free` denies every allocation symbol, defined or referenced, in a
//! linked image. `unified` permits allocation through the platform backend
//! only (RFC-0034 D6's single funnel). That is a rule about which OBJECT
//! references the allocator, and a linked image has thrown that away, so
//! `unified` reads objects and archives, never an image.
//!
//! The symbols are read from the file itself (ELF, its archives and their
//! members), so no `nm` is needed and a cross-built image reads like a host
//! one. A file with no symbol table, or a path with no objects under it, is
//! [`Vacuous`]: a check with nothing to check is never a pass.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::LazyLock,
};

use object::{Object, ObjectSymbol, read::archive::ArchiveFile};
use regex::Regex;
use serde::Serialize;

/// The RTOS kernels' allocator NAMING SCHEME, one alternative per API
/// family (issue 1616). Both checks that ask "does this reach a kernel
/// allocator directly?" read it from here: `nros image check --no-alloc`
/// over a linked image, and `check-no-direct-kernel-alloc.sh` over source,
/// through the `grep -E` pattern [`kernel_alloc_ere`] writes to
/// [`KERNEL_ALLOC_ERE_FILE`].
///
/// * Zephyr: `k_malloc` `k_calloc` `k_realloc` `k_free` `k_aligned_alloc`,
///   `k_heap_*alloc` / `k_heap_free` / `k_heap_realloc`, `sys_heap_*`
/// * FreeRTOS: `pvPortMalloc` `pvPortCalloc` `vPortFree` (and the `Stack`
///   variants)
/// * ThreadX: `tx_byte_allocate` / `tx_byte_release` (and `_tx_`, `_txe_`)
/// * NuttX: `kmm_malloc` `kmm_calloc` `kmm_realloc` `kmm_zalloc`
///   `kmm_memalign` `kmm_free`
/// * ESP-IDF: `heap_caps_*malloc|calloc|realloc|free`
///
/// Each alternative uses `(?:…)` groups only, so it reads as a Rust regex
/// and, with `(?:` rewritten to `(`, as an ERE.
pub const KERNEL_ALLOC_SCHEME: &[&str] = &[
    r"k_(?:heap_)?(?:aligned_)?(?:malloc|calloc|realloc|alloc|free)",
    r"sys_heap_(?:aligned_)?(?:alloc|realloc|free)",
    r"pvPort(?:Malloc|Calloc)(?:Stack)?",
    r"vPortFree(?:Stack)?",
    r"_?txe?_byte_(?:allocate|release)",
    r"kmm_(?:malloc|calloc|realloc|zalloc|memalign|free)",
    r"heap_caps_(?:aligned_)?(?:malloc|calloc|realloc|free|alloc)(?:_prefer)?",
];

/// Where the source check reads [`kernel_alloc_ere`], relative to the repo
/// root. Generated from [`KERNEL_ALLOC_SCHEME`]; a test holds it equal.
pub const KERNEL_ALLOC_ERE_FILE: &str = "scripts/lib/kernel-alloc.ere";

/// The scheme as one `grep -E` pattern, word-bounded.
#[must_use]
pub fn kernel_alloc_ere() -> String {
    let alts: Vec<String> = KERNEL_ALLOC_SCHEME
        .iter()
        .map(|a| a.replace("(?:", "("))
        .collect();
    format!(r"\b({})\b", alts.join("|"))
}

static KERNEL_ALLOC_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!("^(?:{})$", KERNEL_ALLOC_SCHEME.join("|"))).expect("the scheme compiles")
});

/// Whether `name` is one of the kernel allocators [`KERNEL_ALLOC_SCHEME`]
/// describes.
#[must_use]
pub fn is_kernel_alloc(name: &str) -> bool {
    KERNEL_ALLOC_RE.is_match(name)
}

/// The libc heap and its aliases, matched whole.
pub const C_HEAP: &[&str] = &[
    "malloc",
    "calloc",
    "realloc",
    "free",
    "reallocarray",
    "aligned_alloc",
    "posix_memalign",
    "memalign",
    "valloc",
    "pvalloc",
    "strdup",
    "strndup",
    "asprintf",
    "vasprintf",
    "__libc_malloc",
    "__libc_calloc",
    "__libc_realloc",
    "__libc_free",
    "__libc_memalign",
    "_malloc_r",
    "_calloc_r",
    "_realloc_r",
    "_free_r",
    "_memalign_r",
];

static RUST_ALLOC_RE: LazyLock<Regex> = LazyLock::new(|| {
    // `__rust_*` (the compiler's allocator ABI), `__rg_*` (a
    // `#[global_allocator]`) and `__rdl_*` (the default shim), not followed
    // by more of an identifier; and the no-alloc-shim marker by prefix, since
    // it gained a `_v2` suffix and the next rename must not un-check it.
    Regex::new(
        r"(?:__(?:rust|rg|rdl)_(?:alloc_error_handler|alloc_zeroed|dealloc|realloc|alloc|oom)(?:[^A-Za-z0-9_]|$))|__rust_no_alloc_shim_is_unstable",
    )
    .expect("the Rust allocator pattern compiles")
});

/// Itanium mangling of `operator new`, `new[]`, `delete`, `delete[]`.
pub const CXX_MANGLED_PREFIXES: &[&str] = &["_Znw", "_Zna", "_Zdl", "_Zda"];

/// RFC-0034 D6's funnel: not an allocation symbol to deny, the symbol the
/// `unified` tier routes everything else through.
pub const FUNNEL: &[&str] = &[
    "nros_platform_alloc",
    "nros_platform_dealloc",
    "nros_platform_realloc",
];

/// `unified` tier: the objects permitted to reference an allocator directly,
/// each with the reason, which is printed.
pub const PLATFORM_OBJECTS: &[(&str, &str)] = &[
    (
        "*nros[-_]platform*",
        "the platform backend itself, the one funnel every allocation goes through",
    ),
    (
        "*zpico_platform_aliases*",
        "zenoh-pico's `z_malloc`/`z_free` shim, which forwards to \
         `nros_platform_alloc` (its only undefined allocation symbol is the funnel)",
    ),
];

/// An allocation family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Family {
    #[serde(rename = "c-heap")]
    CHeap,
    #[serde(rename = "cxx-operator-new")]
    CxxOperatorNew,
    #[serde(rename = "rtos-heap")]
    RtosHeap,
    #[serde(rename = "rust-alloc-shim")]
    RustAllocShim,
}

impl Family {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Family::CHeap => "c-heap",
            Family::CxxOperatorNew => "cxx-operator-new",
            Family::RtosHeap => "rtos-heap",
            Family::RustAllocShim => "rust-alloc-shim",
        }
    }
}

/// A name without its `@GLIBC_2.2.5` / `@@GLIBC_2.2.5` version suffix.
#[must_use]
pub fn normalise(name: &str) -> &str {
    let n = name.trim();
    match n.find('@') {
        Some(i) if i > 0 => &n[..i],
        _ => n,
    }
}

/// The path segments of an Itanium-mangled name (a nested name, a
/// `std::` one such as `_ZSt4freePv`, or a bare `_Z…`), or `None` for a name
/// that is not one. Rust's
/// legacy mangling is the same shape with a trailing `17h<hash>` segment,
/// which is dropped. Enough to see through a namespace, not a demangler.
#[must_use]
pub fn mangled_path(name: &str) -> Option<Vec<String>> {
    if name.starts_with("_R") {
        return v0_leaf(name).map(|l| vec![l.to_string()]);
    }
    let rest = name.strip_prefix("_Z")?;
    let (mut s, mut out, nested) = if let Some(r) = rest.strip_prefix("N") {
        (r, Vec::new(), true)
    } else if let Some(r) = rest.strip_prefix("St") {
        (r, vec!["std".to_string()], false)
    } else {
        (rest, Vec::new(), false)
    };
    loop {
        // CV-qualifiers and `St` may lead a nested name.
        if nested {
            s = s.trim_start_matches(['K', 'V', 'r']);
            if let Some(r) = s.strip_prefix("St") {
                out.push("std".to_string());
                s = r;
            }
        }
        let digits = s.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            break;
        }
        let len: usize = s[..digits].parse().ok()?;
        let id = s.get(digits..digits + len)?;
        out.push(id.to_string());
        s = &s[digits + len..];
        if !nested || s.starts_with('E') {
            break;
        }
    }
    if out.last().is_some_and(|h| {
        h.len() == 17 && h.starts_with('h') && h[1..].bytes().all(|b| b.is_ascii_hexdigit())
    }) {
        out.pop();
    }
    (!out.is_empty()).then_some(out)
}

/// The last identifier of a Rust v0-mangled path (a Rust function named
/// `strdup` ends in `6strdup`): the `<length>[_]<ident>` that ends the
/// name. A name that does not end in one (a generic instance) gives `None`.
fn v0_leaf(name: &str) -> Option<&str> {
    let b = name.as_bytes();
    for start in 2..b.len() {
        if !b[start].is_ascii_digit() || b[start - 1].is_ascii_digit() {
            continue;
        }
        let digits = b[start..].iter().take_while(|c| c.is_ascii_digit()).count();
        let Ok(len) = name[start..start + digits].parse::<usize>() else {
            continue;
        };
        let mut at = start + digits;
        // An identifier that starts with `_` or a digit is preceded by `_`.
        if b.len() - at == len + 1 && b.get(at) == Some(&b'_') {
            at += 1;
        }
        if len > 0 && b.len() - at == len {
            return Some(&name[at..]);
        }
    }
    None
}

/// The allocation family a symbol belongs to, or `None`.
#[must_use]
pub fn classify(name: &str) -> Option<Family> {
    let n = normalise(name);
    let path = mangled_path(n);
    let leaf = path
        .as_ref()
        .and_then(|p| p.last())
        .map_or(n, String::as_str);
    if C_HEAP.contains(&n) || (path.is_some() && C_HEAP.contains(&leaf)) {
        return Some(Family::CHeap);
    }
    if is_kernel_alloc(n) || (path.is_some() && is_kernel_alloc(leaf)) {
        return Some(Family::RtosHeap);
    }
    if RUST_ALLOC_RE.is_match(n) || (path.is_some() && RUST_ALLOC_RE.is_match(leaf)) {
        return Some(Family::RustAllocShim);
    }
    if CXX_MANGLED_PREFIXES.iter().any(|p| n.starts_with(p)) {
        return Some(Family::CxxOperatorNew);
    }
    None
}

/// The name a report prints: a mangled path joined with `::`, else as read.
#[must_use]
pub fn display_name(name: &str) -> String {
    let n = normalise(name);
    match mangled_path(n) {
        Some(p) if p.len() > 1 => format!("{} ({n})", p.join("::")),
        _ => n.to_string(),
    }
}

/// A check that could not be run. Never a pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vacuous(pub String);

impl std::fmt::Display for Vacuous {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Vacuous {}

/// One symbol as a file's table states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sym {
    pub name: String,
    /// The file calls it and the definition comes from elsewhere.
    pub undefined: bool,
}

fn read_syms<'d, S: ObjectSymbol<'d>>(it: impl Iterator<Item = S>) -> Vec<Sym> {
    it.filter_map(|s| {
        let name = s.name().ok()?;
        (!name.is_empty()).then(|| Sym {
            name: name.to_string(),
            undefined: s.is_undefined(),
        })
    })
    .collect()
}

fn symbols_of(file: &object::File<'_>) -> Vec<Sym> {
    let syms = read_syms(file.symbols());
    if syms.is_empty() {
        // A stripped binary still has a dynamic table.
        read_syms(file.dynamic_symbols())
    } else {
        syms
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>, Vacuous> {
    std::fs::read(path).map_err(|e| Vacuous(format!("{}: {e}", path.display())))
}

/// Every symbol of the linked image at `path`, defined and undefined.
///
/// # Errors
///
/// [`Vacuous`] when the file cannot be read, is not an object file, or has
/// no symbol table: a stripped image cannot be checked.
pub fn image_symbols(path: &Path) -> Result<Vec<Sym>, Vacuous> {
    let data = read_file(path)?;
    let file = object::File::parse(&*data).map_err(|e| {
        Vacuous(format!(
            "{} is not an image this check reads ({e})",
            path.display()
        ))
    })?;
    let syms = symbols_of(&file);
    if syms.is_empty() {
        return Err(Vacuous(format!(
            "{} has no symbol table. A stripped image cannot be checked; build with \
             symbols, or check the image before it is stripped",
            path.display()
        )));
    }
    Ok(syms)
}

/// One allocation symbol in a linked image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageFinding {
    pub symbol: String,
    pub family: Family,
    /// `referenced` (the image calls it, defined elsewhere) or `defined` (the
    /// image contains it): a dependency to look at, or the link.
    pub linkage: &'static str,
}

/// The verdict on one linked image at tier `heap-free`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageReport {
    pub path: PathBuf,
    pub tier: &'static str,
    pub symbols_read: usize,
    /// Whether the image defines the platform funnel: a finding then means a
    /// `unified` image checked as `heap-free`, not a bypass.
    pub funnel_present: bool,
    pub allowed: Vec<String>,
    pub findings: Vec<ImageFinding>,
}

/// Tier `heap-free` over symbols already read: every allocation symbol is a
/// finding, unless `allows` names it.
#[must_use]
pub fn judge_image(path: &Path, syms: &[Sym], allows: &[String]) -> ImageReport {
    let mut findings = Vec::new();
    let mut funnel_present = false;
    for s in syms {
        let n = normalise(&s.name);
        if FUNNEL.contains(&n) && !s.undefined {
            funnel_present = true;
        }
        let Some(family) = classify(&s.name) else {
            continue;
        };
        if allows.iter().any(|a| a == n) {
            continue;
        }
        findings.push(ImageFinding {
            symbol: display_name(&s.name),
            family,
            linkage: if s.undefined { "referenced" } else { "defined" },
        });
    }
    findings.sort_by(|a, b| (a.family, &a.symbol).cmp(&(b.family, &b.symbol)));
    findings.dedup();
    let mut allowed = allows.to_vec();
    allowed.sort();
    ImageReport {
        path: path.to_path_buf(),
        tier: "heap-free",
        symbols_read: syms.len(),
        funnel_present,
        allowed,
        findings,
    }
}

/// Tier `heap-free` on the linked image at `path`.
///
/// # Errors
///
/// As [`image_symbols`].
pub fn check_image(path: &Path, allows: &[String]) -> Result<ImageReport, Vacuous> {
    Ok(judge_image(path, &image_symbols(path)?, allows))
}

/// Every `.o` / `.a` under each of `paths` (a file is taken as given), sorted.
/// The caller passes a build directory: objects are untracked build output.
#[must_use]
pub fn expand_objects(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            for e in walkdir::WalkDir::new(p).into_iter().filter_map(Result::ok) {
                let f = e.path();
                if e.file_type().is_file()
                    && f.extension()
                        .and_then(|x| x.to_str())
                        .is_some_and(|x| matches!(x, "o" | "a" | "obj" | "lib"))
                {
                    out.push(f.to_path_buf());
                }
            }
        } else if p.exists() {
            out.push(p.clone());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// The undefined symbols of each object in `data`: the file itself, or each
/// member of an archive, as `(object, symbols)` with the member spelled
/// `<archive>:<member>`.
fn object_undefined(path: &Path, data: &[u8]) -> Vec<(String, Vec<String>)> {
    let undefined = |f: &object::File<'_>| -> Vec<String> {
        f.symbols()
            .filter(|s| s.is_undefined())
            .filter_map(|s| s.name().ok().map(str::to_string))
            .filter(|n| !n.is_empty())
            .collect()
    };
    let shown = path.display().to_string();
    if let Ok(archive) = ArchiveFile::parse(data) {
        return archive
            .members()
            .filter_map(Result::ok)
            .filter_map(|m| {
                let name = String::from_utf8_lossy(m.name()).to_string();
                let bytes = m.data(data).ok()?;
                let f = object::File::parse(bytes).ok()?;
                Some((format!("{shown}:{name}"), undefined(&f)))
            })
            .collect();
    }
    match object::File::parse(data) {
        Ok(f) => vec![(shown, undefined(&f))],
        Err(_) => Vec::new(),
    }
}

/// `*` in `pattern` matches any run of characters, `[ab]` one of them;
/// everything else matches itself.
fn glob_match(pattern: &str, text: &str) -> bool {
    fn go(p: &[u8], t: &[u8]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some(b'*') => (0..=t.len()).any(|i| go(&p[1..], &t[i..])),
            Some(b'[') => {
                let Some(end) = p.iter().position(|&c| c == b']') else {
                    return t.first() == Some(&b'[') && go(&p[1..], &t[1..]);
                };
                !t.is_empty() && p[1..end].contains(&t[0]) && go(&p[end + 1..], &t[1..])
            }
            Some(&c) => t.first() == Some(&c) && go(&p[1..], &t[1..]),
        }
    }
    go(pattern.as_bytes(), text.as_bytes())
}

/// Why `object` may reference an allocator directly, if it may.
#[must_use]
pub fn object_allowance(object: &str) -> Option<&'static str> {
    PLATFORM_OBJECTS
        .iter()
        .find(|(p, _)| glob_match(p, object))
        .map(|(_, why)| *why)
}

/// One allocator reference from an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObjectFinding {
    pub object: String,
    pub symbol: String,
    pub family: Family,
    /// Set when the object is the platform backend, which may.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_because: Option<&'static str>,
}

/// The verdict on a set of objects at tier `unified`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObjectsReport {
    pub paths: Vec<PathBuf>,
    pub tier: &'static str,
    pub objects_found: usize,
    pub objects_scanned: usize,
    pub findings: Vec<ObjectFinding>,
    pub allowed: Vec<ObjectFinding>,
}

/// One finding per (directory, member, family, symbol): a cargo `out/`
/// directory holds both the loose objects and the archive made of them. The
/// archive spelling is kept, it being what the link consumes. Two archives
/// that each hold a `platform.o` stay apart.
fn dedupe_members(rows: Vec<ObjectFinding>) -> Vec<ObjectFinding> {
    let mut seen: BTreeMap<(String, String, Family, String), ObjectFinding> = BTreeMap::new();
    for r in rows {
        let (holder, member) = match r.object.rsplit_once(':') {
            Some((h, m)) => (h, m),
            None => (r.object.as_str(), r.object.as_str()),
        };
        let dir = Path::new(holder)
            .parent()
            .map(|d| d.display().to_string())
            .unwrap_or_default();
        let base = Path::new(member)
            .file_name()
            .map(|b| b.to_string_lossy().to_string())
            .unwrap_or_default();
        let key = (dir, base, r.family, r.symbol.clone());
        let archive = r.object.contains(':');
        if archive || !seen.contains_key(&key) {
            seen.insert(key, r);
        }
    }
    let mut out: Vec<ObjectFinding> = seen.into_values().collect();
    out.sort_by(|a, b| (&a.object, a.family, &a.symbol).cmp(&(&b.object, b.family, &b.symbol)));
    out
}

/// Tier `unified` over the objects and archives under `paths`: only the
/// platform backend may reference an allocator.
///
/// # Errors
///
/// [`Vacuous`] when `paths` hold no object, or none has a symbol table.
pub fn check_objects(paths: &[PathBuf], allows: &[String]) -> Result<ObjectsReport, Vacuous> {
    let files = expand_objects(paths);
    if files.is_empty() {
        return Err(Vacuous(format!(
            "no .o or .a under {}. Build the image first; a check with nothing to \
             check is not a pass",
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    let (mut findings, mut allowed, mut scanned) = (Vec::new(), Vec::new(), 0);
    for f in &files {
        let Ok(data) = std::fs::read(f) else { continue };
        let objects = object_undefined(f, &data);
        if objects.is_empty() {
            continue;
        }
        scanned += 1;
        for (object, names) in objects {
            for name in names {
                let n = normalise(&name);
                let Some(family) = classify(&name) else {
                    continue;
                };
                if allows.iter().any(|a| a == n) {
                    continue;
                }
                let why = object_allowance(&object);
                let row = ObjectFinding {
                    object: object.clone(),
                    symbol: display_name(&name),
                    family,
                    allowed_because: why,
                };
                if why.is_some() {
                    allowed.push(row);
                } else {
                    findings.push(row);
                }
            }
        }
    }
    if scanned == 0 {
        return Err(Vacuous(format!(
            "none of the {} file(s) found is an object with a symbol table",
            files.len()
        )));
    }
    Ok(ObjectsReport {
        paths: paths.to_vec(),
        tier: "unified",
        objects_found: files.len(),
        objects_scanned: scanned,
        findings: dedupe_members(findings),
        allowed: dedupe_members(allowed),
    })
}

/// The text report of an image.
#[must_use]
pub fn render_image(r: &ImageReport) -> String {
    let mut out = format!(
        "{}\n  tier heap-free, {} symbols read\n",
        r.path.display(),
        r.symbols_read
    );
    if r.findings.is_empty() {
        out.push_str("\nOK: no allocation symbol is linked.\n");
        return out;
    }
    out.push_str(&format!(
        "\nFAIL: {} allocation symbol(s) in an image checked as heap-free.\n",
        r.findings.len()
    ));
    let mut family = None;
    for f in &r.findings {
        if family != Some(f.family) {
            out.push_str(&format!("\n  {}:\n", f.family.as_str()));
            family = Some(f.family);
        }
        out.push_str(&format!("    {:<10}  {}\n", f.linkage, f.symbol));
    }
    out.push('\n');
    out.push_str(if r.funnel_present {
        "The image defines `nros_platform_alloc`: it allocates through the platform \
         on purpose, so check it with `--tier unified` over its objects instead.\n"
    } else {
        "The image does not define `nros_platform_alloc`: these reach a heap \
         outside the platform's allocator.\n"
    });
    out
}

/// The text report of a set of objects.
#[must_use]
pub fn render_objects(r: &ObjectsReport) -> String {
    let mut out = format!(
        "{}\n  tier unified, {} of {} object file(s) read\n",
        r.paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        r.objects_scanned,
        r.objects_found
    );
    if !r.allowed.is_empty() {
        out.push_str("\n  allowed (the platform backend):\n");
        let mut last: Option<(&str, &str)> = None;
        for a in &r.allowed {
            let why = a.allowed_because.unwrap_or_default();
            if last == Some((a.object.as_str(), why)) {
                continue;
            }
            last = Some((a.object.as_str(), why));
            out.push_str(&format!("    {}\n      {why}\n", a.object));
        }
    }
    if r.findings.is_empty() {
        out.push_str("\nOK: every allocator reference comes from the platform backend.\n");
        return out;
    }
    out.push_str(&format!(
        "\nFAIL: {} allocator reference(s) bypass `nros_platform_alloc`:\n",
        r.findings.len()
    ));
    let mut object = None;
    for f in &r.findings {
        if object != Some(f.object.as_str()) {
            out.push_str(&format!("\n  {}\n", f.object));
            object = Some(f.object.as_str());
        }
        out.push_str(&format!("    {:<18}  {}\n", f.family.as_str(), f.symbol));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sym(name: &str, undefined: bool) -> Sym {
        Sym {
            name: name.to_string(),
            undefined,
        }
    }

    fn image(extra: &[(&str, bool)], allows: &[&str]) -> ImageReport {
        let mut syms = vec![
            sym("main", false),
            sym("_ZN9nros_core4init17h0123456789abcdefE", false),
            sym("memcpy", true),
        ];
        syms.extend(extra.iter().map(|(n, u)| sym(n, *u)));
        let allows: Vec<String> = allows.iter().map(|s| s.to_string()).collect();
        judge_image(Path::new("synthetic"), &syms, &allows)
    }

    #[test]
    fn a_heap_free_image_passes() {
        assert!(image(&[], &[]).findings.is_empty());
    }

    #[test]
    fn each_family_alone_fails_an_image() {
        for (name, family) in [
            ("malloc", Family::CHeap),
            ("free@GLIBC_2.2.5", Family::CHeap),
            ("_malloc_r", Family::CHeap),
            ("_ZSt4freePv", Family::CHeap),
            ("_ZN3std4freeEPv", Family::CHeap),
            ("__rust_alloc", Family::RustAllocShim),
            ("__rust_no_alloc_shim_is_unstable", Family::RustAllocShim),
            ("__rust_no_alloc_shim_is_unstable_v2", Family::RustAllocShim),
            ("__rg_dealloc", Family::RustAllocShim),
            (
                "_RNvCs9wFQrvczXsK_7___rustc12___rust_alloc",
                Family::RustAllocShim,
            ),
            (
                "_RNvCs9wFQrvczXsK_7___rustc11___rdl_alloc",
                Family::RustAllocShim,
            ),
            (
                "_ZN7__rustc12__rust_alloc17h0123456789abcdefE",
                Family::RustAllocShim,
            ),
            ("_Znwm", Family::CxxOperatorNew),
            ("_ZdaPvSt11align_val_t", Family::CxxOperatorNew),
            ("_ZdlPv", Family::CxxOperatorNew),
            ("k_malloc", Family::RtosHeap),
            ("pvPortMalloc", Family::RtosHeap),
            ("tx_byte_allocate", Family::RtosHeap),
        ] {
            let r = image(&[(name, true)], &[]);
            assert_eq!(r.findings.len(), 1, "{name} must fail: {r:?}");
            assert_eq!(r.findings[0].family, family, "{name}");
        }
    }

    #[test]
    fn near_misses_do_not_fire() {
        for name in [
            "freeaddrinfo",
            "freeifaddrs",
            "z_free",
            "_z_list_free",
            "nros_platform_alloc",
            "nros_platform_dealloc",
            "free_list_head",
            "mallocinfo",
            "_ZN4core3ptr13drop_in_place17h0123456789abcdefE",
            "_z_string_preallocate",
            "_z_slice_is_alloced",
            "_ZN8heapless4pool4Node3new17h0123456789abcdefE",
            "k_sleep",
            "tx_thread_create",
        ] {
            assert!(
                image(&[(name, true)], &[]).findings.is_empty(),
                "{name} must not fire"
            );
        }
    }

    #[test]
    fn linkage_and_an_allowance_are_reported_as_such() {
        assert_eq!(
            image(&[("malloc", true)], &[]).findings[0].linkage,
            "referenced"
        );
        assert_eq!(
            image(&[("malloc", false)], &[]).findings[0].linkage,
            "defined"
        );
        let two = image(&[("malloc", true), ("k_malloc", true)], &["malloc"]);
        assert_eq!(two.findings.len(), 1);
        assert_eq!(two.allowed, ["malloc"]);
        assert!(image(&[("nros_platform_alloc", false)], &[]).funnel_present);
        assert!(!image(&[("nros_platform_alloc", true)], &[]).funnel_present);
    }

    #[test]
    fn the_kernel_scheme_matches_the_whole_api_and_nothing_near_it() {
        for yes in [
            "k_malloc",
            "k_calloc",
            "k_realloc",
            "k_free",
            "k_aligned_alloc",
            "k_heap_alloc",
            "k_heap_aligned_alloc",
            "k_heap_free",
            "sys_heap_alloc",
            "pvPortMalloc",
            "pvPortCalloc",
            "vPortFree",
            "pvPortMallocStack",
            "tx_byte_allocate",
            "_tx_byte_release",
            "_txe_byte_allocate",
            "kmm_zalloc",
            "heap_caps_realloc",
            "heap_caps_malloc",
        ] {
            assert!(is_kernel_alloc(yes), "{yes}");
        }
        for no in [
            "k_sleep",
            "task_free",
            "zsock_freeaddrinfo",
            "malloc",
            "k_mutex_lock",
            "nros_platform_alloc",
            "tx_thread_create",
        ] {
            assert!(!is_kernel_alloc(no), "{no}");
        }
    }

    /// The ERE file the source check reads is this scheme, written out. To
    /// regenerate: `NROS_UPDATE_GOLDEN=1 cargo test -p nros-cli-core --lib
    /// alloc_scan`.
    #[test]
    fn the_source_check_reads_this_scheme() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../..")
            .join(KERNEL_ALLOC_ERE_FILE);
        let want = format!("{}\n", kernel_alloc_ere());
        if std::env::var("NROS_UPDATE_GOLDEN").is_ok_and(|v| v != "0") {
            std::fs::write(&path, &want).unwrap();
        }
        let got = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            got,
            want,
            "{} is stale: NROS_UPDATE_GOLDEN=1 cargo test -p nros-cli-core --lib alloc_scan",
            path.display()
        );
        // The ERE reads as a regex too, with the same answers.
        let ere = Regex::new(&kernel_alloc_ere()).unwrap();
        assert!(ere.is_match("return k_realloc(p, 8);"));
        assert!(ere.is_match("#define X(n) pvPortMalloc(n)"));
        assert!(!ere.is_match("k_sleep(1)"));
        assert!(!ere.is_match("my_k_malloc_wrapper("));
    }

    #[test]
    fn a_mangled_path_is_read_and_a_rust_hash_dropped() {
        assert_eq!(mangled_path("_ZSt4freePv").unwrap(), ["std", "free"]);
        assert_eq!(mangled_path("_ZN3std4freeEPv").unwrap(), ["std", "free"]);
        assert_eq!(
            mangled_path("_ZN7__rustc12__rust_alloc17h0123456789abcdefE").unwrap(),
            ["__rustc", "__rust_alloc"]
        );
        assert_eq!(mangled_path("malloc"), None);
        // Rust v0: the leaf, so a Rust `strdup` is caught as the old `nm -C`
        // read did (a false positive is loud; a miss is silent).
        assert_eq!(
            mangled_path("_RNvNtCs1a2b3c_14unsafe_libyaml7externs6strdup").unwrap(),
            ["strdup"]
        );
        assert_eq!(
            mangled_path("_RNvCs9wFQrvczXsK_7___rustc12___rust_alloc").unwrap(),
            ["__rust_alloc"]
        );
        assert_eq!(
            classify("_RNvNtCs1a2b3c_14unsafe_libyaml7externs6strdup"),
            Some(Family::CHeap)
        );
        assert_eq!(classify("_RNvNtCs1a2b3c_4core3ptr13drop_in_place"), None);
        assert_eq!(
            display_name("_ZSt4freePv"),
            "std::free (_ZSt4freePv)".to_string()
        );
    }

    #[test]
    fn the_platform_objects_and_only_they_are_allowed() {
        assert!(object_allowance("target/out/libnros_platform_zephyr.a:alloc.o").is_some());
        assert!(object_allowance("build/x/nros-platform-freertos/p.o").is_some());
        assert!(object_allowance("out/libzpico.a:zpico_platform_aliases.o").is_some());
        assert!(object_allowance("out/libzenohpico.a:session.o").is_none());
    }

    #[test]
    fn an_archive_member_and_its_loose_object_are_one_finding() {
        let row = |object: &str| ObjectFinding {
            object: object.to_string(),
            symbol: "malloc".to_string(),
            family: Family::CHeap,
            allowed_because: None,
        };
        let rows = dedupe_members(vec![
            row("out/foo.o"),
            row("out/libx.a:foo.o"),
            row("other/liby.a:foo.o"),
        ]);
        assert_eq!(
            rows.iter().map(|r| r.object.as_str()).collect::<Vec<_>>(),
            ["other/liby.a:foo.o", "out/libx.a:foo.o"]
        );
    }

    #[test]
    fn nothing_to_check_is_vacuous_never_a_pass() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(check_objects(&[tmp.path().join("nope")], &[]).is_err());
        assert!(check_objects(&[tmp.path().to_path_buf()], &[]).is_err());
        let text = tmp.path().join("not-an-image");
        std::fs::write(&text, "plain text").unwrap();
        assert!(check_image(&text, &[]).is_err());
        assert!(check_image(&tmp.path().join("missing"), &[]).is_err());
    }

    /// The reader on a real image: the test binary itself links the Rust
    /// allocator and libc's `malloc`.
    #[test]
    fn a_real_image_is_read() {
        let exe = std::env::current_exe().unwrap();
        let r = check_image(&exe, &[]).unwrap();
        assert!(r.symbols_read > 100, "{}", r.symbols_read);
        assert!(
            r.findings.iter().any(|f| f.family == Family::RustAllocShim),
            "{}",
            render_image(&r)
        );
    }
}
