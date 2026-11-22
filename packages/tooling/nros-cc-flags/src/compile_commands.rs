//! Every cc-rs compile of image C, with a `compile_commands.json` beside it.
//!
//! # Why
//!
//! The stack-floor check types each indirect call in the image's C from
//! clang's IR of the same translation unit, which it compiles from the
//! build's compile commands (issue 2516). A cmake or west build writes those
//! commands; cargo does not, so C that a build script compiled with cc-rs had
//! none, and every indirect call in it was bound to every function C can
//! hold. On the realtime-c FreeRTOS image, giving zenoh-pico's commands to the
//! walk alone took 18-21 % off it (issue 2658).
//!
//! # The mechanism
//!
//! [`compile`] is the one way a build script compiles image C. It gives the
//! compile its own object directory, `$OUT_DIR/cc/<lib>/`, so every depfile
//! there is this compile's. It adds `-MMD` ([`crate::header_deps`]), compiles,
//! and writes that directory's `compile_commands.json`.
//!
//! - **Command:** the one cc-rs ran: the compiler and arguments from
//!   `cc::Build::get_compiler`, then `-c <source> -o <object>`.
//! - **Pairing:** the object and its source are paired by the compiler's own
//!   depfile (`<obj>.d`, whose first prerequisite is the source). Nothing
//!   here re-derives cc-rs's object naming.
//! - **Object identity:** each entry also records its object's size and
//!   FNV-1a-64 hash (`nros_object`). The walk accepts an entry only for the
//!   linked object with those bytes, so a stale `OUT_DIR` from another
//!   configuration, with the same object names and different flags, is never
//!   read for this image. No file time is involved.
//!
//! `check-cc-compile-commands` refuses a build-script `.compile(` that does
//! not go through [`compile`].

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::header_deps::parse_depfile;

/// The fragment's name inside a compile's object directory.
pub const FRAGMENT: &str = "compile_commands.json";

/// The object directory [`compile`] gives library `lib` under `out_dir`.
pub fn lib_dir(out_dir: &Path, lib: &str) -> PathBuf {
    out_dir.join("cc").join(lib)
}

/// Compile `build` as the static library `lib`, exactly as `build.compile(lib)`
/// would, in its own object directory, and write that directory's
/// [`FRAGMENT`]. Returns the directory, which holds `lib<lib>.a` (cc-rs
/// already emitted its `rustc-link-search`).
///
/// The caller still calls [`crate::header_deps::emit_header_deps`] on
/// `$OUT_DIR` after its compiles: it reads the depfiles recursively.
pub fn compile(build: &mut cc::Build, lib: &str) -> PathBuf {
    let out_dir =
        PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR: called outside a build script"));
    let dir = lib_dir(&out_dir, lib);
    fs::create_dir_all(&dir).expect("create the compile's object directory");
    // Depfiles a previous run left here belong to sources this compile may no
    // longer have; only the compile below may name what is in the directory.
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "d") {
                let _ = fs::remove_file(e.path());
            }
        }
    }
    crate::header_deps::track_header_deps(build);
    build.out_dir(&dir);
    build.compile(lib);
    write(build, &dir);
    dir
}

/// FNV-1a, 64-bit: the hash an entry records for its object. The walker
/// computes the same over the linked object's bytes.
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// One compile: the source, the object, its identity and the full argument
/// vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub file: PathBuf,
    pub output: PathBuf,
    pub object_size: u64,
    pub object_fnv1a64: u64,
    pub arguments: Vec<String>,
}

/// The entries for every `<x>.o` with a `<x>.d` directly in `dir`: `compiler`
/// and `args` (the compiler's arguments, without the source and object),
/// then `-c <source> -o <object>`. A depfile with no prerequisite, or an
/// object that cannot be read, is skipped. Sorted by object.
pub fn entries(dir: &Path, compiler: &str, args: &[String]) -> Vec<Entry> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let d = e.path();
        if d.extension().is_none_or(|x| x != "d") {
            continue;
        }
        let obj = d.with_extension("o");
        let (Ok(text), Ok(bytes)) = (fs::read_to_string(&d), fs::read(&obj)) else {
            continue;
        };
        let Some(src) = parse_depfile(&text).into_iter().next() else {
            continue;
        };
        let src = PathBuf::from(src);
        let mut arguments = Vec::with_capacity(args.len() + 5);
        arguments.push(compiler.to_owned());
        arguments.extend(args.iter().cloned());
        arguments.extend([
            "-c".to_owned(),
            src.display().to_string(),
            "-o".to_owned(),
            obj.display().to_string(),
        ]);
        out.push(Entry {
            file: src,
            output: obj,
            object_size: bytes.len() as u64,
            object_fnv1a64: fnv1a64(&bytes),
            arguments,
        });
    }
    out.sort_by(|a, b| a.output.cmp(&b.output));
    out
}

fn json_str(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// `entries` as a compile_commands.json document, `directory` = `dir`.
pub fn to_json(dir: &Path, entries: &[Entry]) -> String {
    let d = json_str(&dir.display().to_string());
    let rows: Vec<String> = entries
        .iter()
        .map(|e| {
            let args: Vec<String> = e.arguments.iter().map(|a| json_str(a)).collect();
            format!(
                "  {{\"directory\": {d}, \"file\": {}, \"output\": {}, \
                 \"nros_object\": {{\"size\": {}, \"fnv1a64\": \"{:016x}\"}}, \"arguments\": [{}]}}",
                json_str(&e.file.display().to_string()),
                json_str(&e.output.display().to_string()),
                e.object_size,
                e.object_fnv1a64,
                args.join(", ")
            )
        })
        .collect();
    format!("[\n{}\n]\n", rows.join(",\n"))
}

/// Write `dir`'s [`FRAGMENT`] for the compile `build` just ran there.
/// Write-if-changed. Returns how many commands it holds; 0 and nothing
/// written when the compiler cannot be resolved (outside a build script).
fn write(build: &cc::Build, dir: &Path) -> usize {
    let Ok(tool) = build.try_get_compiler() else {
        return 0;
    };
    let compiler = tool.path().display().to_string();
    let args: Vec<String> = tool
        .args()
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let es = entries(dir, &compiler, &args);
    let text = to_json(dir, &es);
    let path = dir.join(FRAGMENT);
    if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        fs::write(&path, text).expect("write the compile's compile_commands.json");
    }
    es.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nros-cc-cc-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn pairs_each_object_with_the_source_its_depfile_names() {
        let d = tmp("pairs");
        fs::write(d.join("1234-session.o"), b"ab").unwrap();
        fs::write(
            d.join("1234-session.d"),
            "1234-session.o: /src/net/session.c \\\n /src/a.h\n",
        )
        .unwrap();
        fs::write(d.join("5678-session.o"), b"").unwrap();
        fs::write(
            d.join("5678-session.d"),
            "5678-session.o: /src/api/session.c\n",
        )
        .unwrap();
        // a depfile with no object beside it, and an object with no depfile
        fs::write(d.join("orphan.d"), "orphan.o: /src/orphan.c\n").unwrap();
        fs::write(d.join("nodep.o"), b"").unwrap();
        let es = entries(&d, "gcc", &["-O2".into(), "-DX=1".into()]);
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].file, PathBuf::from("/src/net/session.c"));
        assert_eq!(es[1].file, PathBuf::from("/src/api/session.c"));
        assert_eq!(
            (es[0].object_size, es[0].object_fnv1a64),
            (2, fnv1a64(b"ab"))
        );
        assert_eq!(
            es[0].arguments,
            ["gcc", "-O2", "-DX=1", "-c", "/src/net/session.c", "-o"]
                .iter()
                .map(|s| s.to_string())
                .chain([d.join("1234-session.o").display().to_string()])
                .collect::<Vec<_>>()
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn the_hash_is_fnv1a64() {
        // the published FNV-1a-64 test vectors; the walker's copy checks the same
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn the_document_escapes_and_records_each_object() {
        let e = Entry {
            file: "/s/a \"q\".c".into(),
            output: "/o/x.o".into(),
            object_size: 7,
            object_fnv1a64: 0xab,
            arguments: vec!["gcc".into(), "-DS=\"v\\w\"".into()],
        };
        let j = to_json(Path::new("/o"), &[e]);
        assert!(j.contains(r#""file": "/s/a \"q\".c""#), "{j}");
        assert!(j.contains(r#""output": "/o/x.o""#), "{j}");
        assert!(
            j.contains(r#""nros_object": {"size": 7, "fnv1a64": "00000000000000ab"}"#),
            "{j}"
        );
        assert!(j.contains(r#""-DS=\"v\\w\"""#), "{j}");
        assert!(j.starts_with("[\n") && j.ends_with("]\n"));
    }

    #[test]
    fn an_empty_compile_is_an_empty_document() {
        let d = tmp("empty");
        assert!(entries(&d, "gcc", &[]).is_empty());
        assert_eq!(to_json(&d, &[]), "[\n\n]\n");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn each_library_gets_its_own_directory() {
        let a = lib_dir(Path::new("/out"), "zenohpico");
        let b = lib_dir(Path::new("/out"), "zpico");
        assert_ne!(a, b);
        assert_eq!(a, PathBuf::from("/out/cc/zenohpico"));
    }
}
