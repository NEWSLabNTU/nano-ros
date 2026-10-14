//! Issue 1896 — nano-ros's Micro-XRCE-DDS-Client and micro-CDR are compiled
//! under PRIVATE symbol names, so they can share an image with somebody else's
//! copy of the client.
//!
//! MEASURED: a PX4 SITL image built with the uORB bridge on `xrce`
//! (`NROS_PX4_BRIDGE_RMW=xrce`) linked two copies of the client. PX4's stock
//! `uxrce_dds_client` brings its own `libmicroxrcedds_client.a` +
//! `libmicrocdr.a` (client 2.4.0, micro-CDR 2.0.1, `UCLIENT_PROFILE_CUSTOM_TRANSPORT`
//! OFF); `libnros_cpp.a` carried this archive (client 3.0.1, micro-CDR 2.0.2,
//! custom transport ON). 512 strong globals were defined by both (344 `uxr_*`,
//! 168 `ucdr_*`). Two archives defining one strong symbol link WITHOUT a
//! diagnostic when the first archive's member satisfies every reference, so
//! PX4's 2.x client called into our 3.x objects and jumped through a NULL
//! function pointer in `uxr_ping_agent_attempts` before PX4's startup script
//! returned. Same class as issue 0436 (two zenoh-pico copies), one backend over,
//! and nothing about it is PX4-specific: ANY image that links nano-ros's XRCE
//! backend beside another Micro-XRCE client has it.
//!
//! WHAT IS RENAMED IS READ FROM THE OBJECTS, NOT FROM A PREFIX. The first cut of
//! this fix renamed every `uxr_*` / `ucdr_*` identifier and turned the silent
//! collision into a LOUD one: upstream also exports unprefixed globals
//! (`read_submessage_info`, `on_full_output_buffer`, `get_available_free_slots`,
//! `serialize_get_info_message`, …), and once PX4's references stopped being
//! satisfied by our members, ld pulled PX4's and reported 7 multiple
//! definitions. So the vendored trees are compiled once into a probe directory,
//! their ELF symbol tables are read, and the rename set is: every global the
//! vendored objects DEFINE, plus every `uxr_*` / `ucdr_*` they REFERENCE (the
//! clock hooks `uxr_millis`/`uxr_nanos` are defined by our backend TU
//! `platform_aliases.c`, so they are references here). A pasted name
//! (`ucdr_serialize ## SUFFIX`) is in a symbol table like any other, which a
//! grep of the sources is not.
//!
//! THE FIX IS AT COMPILE TIME. The set becomes a force-included header of
//! `#define <name> nros_<name>`, applied to every TU of the archive — vendored
//! and backend alike — so every caller inside it is renamed with it. An
//! `objcopy --redefine-syms` pass would need a target `objcopy` on all six
//! target families and would rewrite an artifact after the compiler made it;
//! reading ELF here needs nothing but the bytes.
//!
//! THE RESULT IS CHECKED ON THE ARTIFACT. After the real compile, [`verify`]
//! reads the archive and fails the build if any symbol in it — defined or
//! referenced — still carries a name from the set or a bare `uxr_`/`ucdr_`
//! prefix.
//!
//! A consumer OUTSIDE this archive that names the client's API must compile
//! with the same header: `nros-rmw-xrce/CMakeLists.txt`'s CTest binaries read
//! its path from the pointer file (`prefix_header=`). Nothing in an image does —
//! every caller of the client lives in this archive.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

/// What every renamed identifier gains.
pub const PREFIX: &str = "nros_";
/// The prefixes the vendored trees own. A REFERENCE carrying one is renamed
/// too: it names a hook this archive defines on the client's behalf.
pub const OWNED: &[&str] = &["uxr_", "ucdr_"];
/// The generated header, under `OUT_DIR/include`.
pub const HEADER: &str = "nros_xrce_symbol_prefix.h";

/// Compile `vendored` with `build`'s flags into a probe directory, derive the
/// rename set from the objects' symbol tables, write the header and return its
/// path with the set. The set is EMPTY when the objects are not ELF (a macOS
/// host): the header then renames nothing, and a warning says so.
pub fn generate(
    build: &cc::Build,
    vendored: &[PathBuf],
    out_dir: &Path,
) -> (PathBuf, BTreeSet<String>) {
    let mut probe = build.clone();
    probe.out_dir(out_dir.join("nros-xrce-symbol-probe"));
    for src in vendored {
        probe.file(src);
    }
    let objects = probe.try_compile_intermediates().unwrap_or_else(|e| {
        panic!("nros-rmw-xrce-cffi: the symbol-prefix probe compile failed (issue 1896): {e}")
    });

    let mut defined = BTreeSet::new();
    let mut referenced = BTreeSet::new();
    let mut elf = true;
    for obj in &objects {
        let bytes = fs::read(obj)
            .unwrap_or_else(|e| panic!("nros-rmw-xrce-cffi: read {} ({e})", obj.display()));
        let Some(syms) = elf_globals(&bytes) else {
            println!(
                "cargo:warning=nros-rmw-xrce-cffi: {} is not an ELF object, so the vendored \
                 Micro-XRCE-DDS-Client is built WITHOUT its private symbol prefix — it will \
                 collide with any other copy of the client in the same image (issue 1896)",
                obj.display()
            );
            elf = false;
            break;
        };
        for s in syms {
            if s.defined {
                defined.insert(s.name);
            } else {
                referenced.insert(s.name);
            }
        }
    }
    let mut names = if elf { defined } else { BTreeSet::new() };
    names.extend(
        referenced
            .into_iter()
            .filter(|n| OWNED.iter().any(|p| n.starts_with(p))),
    );
    assert!(
        !elf || names.contains("uxr_create_session")
            && names.iter().any(|n| n.starts_with("ucdr_serialize_")),
        "nros-rmw-xrce-cffi: the symbol-prefix probe found {} names and not the client's \
         own `uxr_create_session` / micro-CDR's `ucdr_serialize_*` — it is not reading the \
         objects this archive compiles (issue 1896)",
        names.len()
    );
    if let Some(n) = names.iter().find(|n| n.starts_with(PREFIX)) {
        panic!(
            "nros-rmw-xrce-cffi: the vendored client already defines `{n}`, inside the private \
             prefix `{PREFIX}` this archive renames into (issue 1896)"
        );
    }

    let path = out_dir.join("include").join(HEADER);
    let mut text = String::from(
        "/* Generated by nros-rmw-xrce-cffi/build.rs (issue 1896). Do not edit.\n \
         *\n \
         * Force-included into every TU of nano-ros's XRCE archive: every global\n \
         * the vendored Micro-XRCE-DDS-Client / micro-CDR objects define, and every\n \
         * uxr_/ucdr_ hook they reference, compiles under a private nros_ name so\n \
         * it cannot collide with another copy of the client in the same image. */\n\
         #ifndef NROS_XRCE_SYMBOL_PREFIX_H\n\
         #define NROS_XRCE_SYMBOL_PREFIX_H\n",
    );
    for n in &names {
        text.push_str(&format!("#define {n} {PREFIX}{n}\n"));
    }
    text.push_str("#endif /* NROS_XRCE_SYMBOL_PREFIX_H */\n");
    fs::create_dir_all(path.parent().expect("header has a parent"))
        .unwrap_or_else(|e| panic!("nros-rmw-xrce-cffi: mkdir for {} ({e})", path.display()));
    fs::write(&path, text)
        .unwrap_or_else(|e| panic!("nros-rmw-xrce-cffi: cannot write {} ({e})", path.display()));
    (path, names)
}

/// Add the force-include of `header` to `build`.
pub fn force_include(build: &mut cc::Build, header: &Path) {
    if build.get_compiler().is_like_msvc() {
        build.flag(format!("/FI{}", header.display()));
    } else {
        build.flag("-include").flag(header.as_os_str());
    }
}

/// The negative control, on the ARTIFACT: no symbol in `archive` may still
/// carry a name from `names`, or a bare owned prefix.
pub fn verify(archive: &Path, names: &BTreeSet<String>) {
    let bytes = fs::read(archive)
        .unwrap_or_else(|e| panic!("nros-rmw-xrce-cffi: read {} ({e})", archive.display()));
    let members = ar_members(&bytes).unwrap_or_else(|| {
        panic!(
            "nros-rmw-xrce-cffi: {} is not an `ar` archive this script can read (issue 1896)",
            archive.display()
        )
    });
    let mut survivors = BTreeSet::new();
    let mut seen_prefixed = false;
    for (member, data) in members {
        let Some(syms) = elf_globals(data) else {
            continue;
        };
        for s in syms {
            if s.name.starts_with(PREFIX)
                && OWNED.iter().any(|p| s.name[PREFIX.len()..].starts_with(p))
            {
                seen_prefixed = true;
            }
            if names.contains(&s.name) || OWNED.iter().any(|p| s.name.starts_with(p)) {
                survivors.insert(format!("{} ({member})", s.name));
            }
        }
    }
    assert!(
        survivors.is_empty(),
        "nros-rmw-xrce-cffi: {} symbol(s) in {} kept their unprefixed Micro-XRCE-DDS-Client \
         name, so another copy of the client in the image can bind to them (issue 1896):\n  {}",
        survivors.len(),
        archive.display(),
        survivors.into_iter().collect::<Vec<_>>().join("\n  ")
    );
    assert!(
        seen_prefixed,
        "nros-rmw-xrce-cffi: {} holds no `{PREFIX}uxr_*`/`{PREFIX}ucdr_*` symbol at all — the \
         verification read nothing, which would pass vacuously (issue 1896)",
        archive.display()
    );
}

/// One global symbol of an ELF relocatable object.
pub struct Sym {
    pub name: String,
    pub defined: bool,
}

/// Every GLOBAL/WEAK, non-section, non-file symbol of an ELF object. `None`
/// when `b` is not ELF. Handles ELF32/ELF64 in either byte order — the six
/// target families this crate is built for are all ELF.
pub fn elf_globals(b: &[u8]) -> Option<Vec<Sym>> {
    if b.len() < 52 || &b[..4] != b"\x7fELF" {
        return None;
    }
    let is64 = match b[4] {
        1 => false,
        2 => true,
        _ => return None,
    };
    let le = match b[5] {
        1 => true,
        2 => false,
        _ => return None,
    };
    let u16_at = |o: usize| -> Option<u64> {
        let s: [u8; 2] = b.get(o..o + 2)?.try_into().ok()?;
        Some(u64::from(if le {
            u16::from_le_bytes(s)
        } else {
            u16::from_be_bytes(s)
        }))
    };
    let u32_at = |o: usize| -> Option<u64> {
        let s: [u8; 4] = b.get(o..o + 4)?.try_into().ok()?;
        Some(u64::from(if le {
            u32::from_le_bytes(s)
        } else {
            u32::from_be_bytes(s)
        }))
    };
    let u64_at = |o: usize| -> Option<u64> {
        let s: [u8; 8] = b.get(o..o + 8)?.try_into().ok()?;
        Some(if le {
            u64::from_le_bytes(s)
        } else {
            u64::from_be_bytes(s)
        })
    };
    let word = |o: usize| if is64 { u64_at(o) } else { u32_at(o) };
    let (shoff, shentsize, shnum) = if is64 {
        (u64_at(0x28)?, u16_at(0x3A)?, u16_at(0x3C)?)
    } else {
        (u32_at(0x20)?, u16_at(0x2E)?, u16_at(0x30)?)
    };
    let sh = |i: u64| (shoff + i * shentsize) as usize;
    // (sh_type, sh_offset, sh_size, sh_link, sh_entsize)
    let section = |i: u64| -> Option<(u64, u64, u64, u64, u64)> {
        let h = sh(i);
        if is64 {
            Some((
                u32_at(h + 4)?,
                word(h + 24)?,
                word(h + 32)?,
                u32_at(h + 40)?,
                word(h + 56)?,
            ))
        } else {
            Some((
                u32_at(h + 4)?,
                word(h + 16)?,
                word(h + 20)?,
                u32_at(h + 24)?,
                word(h + 36)?,
            ))
        }
    };
    const SHT_SYMTAB: u64 = 2;
    let mut out = Vec::new();
    for i in 0..shnum {
        let (ty, off, size, link, entsize) = section(i)?;
        if ty != SHT_SYMTAB || entsize == 0 {
            continue;
        }
        let (_, stroff, strsize, _, _) = section(link)?;
        let strtab = b.get(stroff as usize..(stroff + strsize) as usize)?;
        for k in 1..size / entsize {
            let e = (off + k * entsize) as usize;
            let (name_off, info, shndx) = if is64 {
                (u32_at(e)?, *b.get(e + 4)?, u16_at(e + 6)?)
            } else {
                (u32_at(e)?, *b.get(e + 12)?, u16_at(e + 14)?)
            };
            let bind = info >> 4;
            let kind = info & 0xf;
            // STB_GLOBAL=1, STB_WEAK=2; STT_SECTION=3, STT_FILE=4.
            if !(bind == 1 || bind == 2) || kind == 3 || kind == 4 {
                continue;
            }
            let rest = strtab.get(name_off as usize..)?;
            let end = rest.iter().position(|&c| c == 0)?;
            let name = String::from_utf8_lossy(&rest[..end]).into_owned();
            if name.is_empty() {
                continue;
            }
            out.push(Sym {
                name,
                defined: shndx != 0,
            });
        }
    }
    Some(out)
}

/// The members of a System V / GNU `ar` archive as `(name, bytes)`. The symbol
/// index and the long-name table are skipped; names are only for messages.
fn ar_members(b: &[u8]) -> Option<Vec<(String, &[u8])>> {
    if !b.starts_with(b"!<arch>\n") {
        return None;
    }
    let mut out = Vec::new();
    let mut o = 8;
    while o + 60 <= b.len() {
        let hdr = &b[o..o + 60];
        let name = String::from_utf8_lossy(&hdr[..16]).trim_end().to_string();
        let size: usize = String::from_utf8_lossy(&hdr[48..58]).trim().parse().ok()?;
        let data = b.get(o + 60..o + 60 + size)?;
        if !(name == "/"
            || name == "//"
            || name.starts_with("/SYM64/")
            || name.starts_with("__.SYMDEF"))
        {
            out.push((name, data));
        }
        o += 60 + size + (size & 1);
    }
    Some(out)
}
