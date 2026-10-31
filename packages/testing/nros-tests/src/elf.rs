//! Read an ELF's section headers and static symbol table IN PROCESS
//! (issue 2122).
//!
//! The one reader the tests share. Two tests read symbols out of prebuilt
//! artifacts: `rmw_coordinate_truth` (which backend did a row link?) and
//! `zephyr_link_layout` (where did the linker put a symbol?). The first ran
//! `nm` once per binary on a pool, and that put a CHILD PROCESS per artifact
//! under a wall-clock budget: on a loaded tier-2 runner one `nm` took 9 s
//! against 0.04 s idle, and on a cold page cache here a sweep over 85
//! binaries took 21.4 s against 0.9 s warm. Neither number was about the
//! artifacts. `nm` sorts, formats and prints every symbol so the caller can
//! parse the text back; this reads only the bytes the question needs (the
//! header, the section headers, `.symtab` and its string table) with
//! positioned reads, so there is no process to start, no pipe to drain and
//! nothing to kill.
//!
//! Both ELF classes and both byte orders, because the artifacts are host
//! (x86_64/aarch64 ELF64 LE) and cross (Cortex-M ELF32 LE, RISC-V ELF64)
//! alike. Anything else is an `Err`, never a guess.

use std::{fs::File, os::unix::fs::FileExt, path::Path};

/// `sh_type` of the static symbol table.
pub const SHT_SYMTAB: u32 = 2;
/// `sh_flags` bit of a section that holds code.
pub const SHF_EXECINSTR: u64 = 0x4;
/// `st_info` binding of a weak symbol (`nm` prints `W`/`w`/`V`/`v`).
pub const STB_WEAK: u8 = 2;
/// `st_info` binding of a GNU unique symbol (`nm` prints `u`).
pub const STB_GNU_UNIQUE: u8 = 10;
/// First reserved section index (`SHN_LORESERVE`): `SHN_ABS`, `SHN_COMMON`,
/// `SHN_XINDEX`... name no section header.
const SHN_LORESERVE: u16 = 0xff00;

/// One section header, with its name resolved.
#[derive(Debug, Clone)]
pub struct Section {
    pub name: String,
    pub kind: u32,
    pub flags: u64,
    pub addr: u64,
    pub offset: u64,
    pub size: u64,
    pub link: u32,
    pub entsize: u64,
}

/// One `.symtab` entry, with its name resolved.
#[derive(Debug, Clone)]
pub struct Symbol {
    pub name: String,
    pub value: u64,
    pub size: u64,
    /// `st_info >> 4`.
    pub bind: u8,
    /// `st_info & 0xf`.
    pub kind: u8,
    /// `0` is undefined (an import); `>= 0xff00` is a reserved index.
    pub shndx: u16,
}

/// The section headers and the static symbol table of one ELF file.
#[derive(Debug, Clone)]
pub struct Elf {
    pub sections: Vec<Section>,
    pub symbols: Vec<Symbol>,
}

impl Elf {
    /// The first DEFINED symbol called `name`.
    pub fn defined(&self, name: &str) -> Option<&Symbol> {
        self.symbols.iter().find(|s| s.name == name && s.shndx != 0)
    }

    /// The section a symbol is defined in, if its index names one.
    pub fn section_of(&self, sym: &Symbol) -> Option<&Section> {
        if sym.shndx == 0 || sym.shndx >= SHN_LORESERVE {
            return None;
        }
        self.sections.get(sym.shndx as usize)
    }

    /// Is `sym` what `nm` prints as `T` or `t` -- a non-weak symbol defined in
    /// a code section? An import (`U`), data (`d`/`D`/`b`/`r`...) and a weak
    /// definition (`W`) are not.
    pub fn is_text(&self, sym: &Symbol) -> bool {
        sym.bind != STB_WEAK
            && sym.bind != STB_GNU_UNIQUE
            && self
                .section_of(sym)
                .is_some_and(|s| s.flags & SHF_EXECINSTR != 0)
    }

    /// Is `sym` what `nm` prints as `U` -- a non-weak UNDEFINED reference? In
    /// a linked executable that is a symbol a shared library must supply at
    /// load time. A weak undefined (`w`) may stay unresolved, so it is not.
    pub fn is_import(&self, sym: &Symbol) -> bool {
        sym.shndx == 0 && sym.bind != STB_WEAK && !sym.name.is_empty()
    }
}

/// Little- or big-endian field reads over a byte slice.
#[derive(Clone, Copy)]
struct Order {
    big: bool,
}

impl Order {
    fn u16(self, b: &[u8], o: usize) -> Result<u16, String> {
        let a: [u8; 2] = field(b, o)?;
        Ok(if self.big {
            u16::from_be_bytes(a)
        } else {
            u16::from_le_bytes(a)
        })
    }
    fn u32(self, b: &[u8], o: usize) -> Result<u32, String> {
        let a: [u8; 4] = field(b, o)?;
        Ok(if self.big {
            u32::from_be_bytes(a)
        } else {
            u32::from_le_bytes(a)
        })
    }
    fn u64(self, b: &[u8], o: usize) -> Result<u64, String> {
        let a: [u8; 8] = field(b, o)?;
        Ok(if self.big {
            u64::from_be_bytes(a)
        } else {
            u64::from_le_bytes(a)
        })
    }
}

fn field<const N: usize>(b: &[u8], o: usize) -> Result<[u8; N], String> {
    b.get(o..o + N)
        .and_then(|s| s.try_into().ok())
        .ok_or_else(|| format!("truncated: {N} bytes at offset {o} past {}", b.len()))
}

fn c_str(table: &[u8], o: usize) -> String {
    let Some(tail) = table.get(o..) else {
        return String::new();
    };
    let end = tail.iter().position(|&c| c == 0).unwrap_or(tail.len());
    String::from_utf8_lossy(&tail[..end]).into_owned()
}

fn read_at(f: &File, offset: u64, len: u64, what: &str) -> Result<Vec<u8>, String> {
    let len = usize::try_from(len).map_err(|_| format!("{what}: {len} bytes"))?;
    let mut buf = vec![0u8; len];
    f.read_exact_at(&mut buf, offset)
        .map_err(|e| format!("{what}: {len} bytes at {offset:#x}: {e}"))?;
    Ok(buf)
}

/// Read `path`'s section headers and every `SHT_SYMTAB` section's symbols.
///
/// `Err` names what was wrong: not an ELF, truncated, or a reserved layout
/// this does not model. A file with no `.symtab` (a stripped image) is `Ok`
/// with no symbols, so the caller decides what an empty table means.
pub fn read(path: &Path) -> Result<Elf, String> {
    let f = File::open(path).map_err(|e| format!("open: {e}"))?;
    let ident = read_at(&f, 0, 16, "ELF ident").map_err(|e| format!("not an ELF ({e})"))?;
    if &ident[..4] != b"\x7fELF" {
        return Err("not an ELF (bad magic)".into());
    }
    let wide = match ident[4] {
        1 => false,
        2 => true,
        c => return Err(format!("unknown ELF class {c}")),
    };
    let ord = Order {
        big: match ident[5] {
            1 => false,
            2 => true,
            d => return Err(format!("unknown ELF data encoding {d}")),
        },
    };
    let ehdr = read_at(&f, 0, if wide { 64 } else { 52 }, "ELF header")?;
    let (shoff, shentsize, shnum, shstrndx) = if wide {
        (
            ord.u64(&ehdr, 40)?,
            ord.u16(&ehdr, 58)?,
            ord.u16(&ehdr, 60)?,
            ord.u16(&ehdr, 62)?,
        )
    } else {
        (
            ord.u32(&ehdr, 32)?.into(),
            ord.u16(&ehdr, 46)?,
            ord.u16(&ehdr, 48)?,
            ord.u16(&ehdr, 50)?,
        )
    };
    if shoff == 0 || shnum == 0 {
        // No section headers: nothing to name and no symbol table.
        return Ok(Elf {
            sections: Vec::new(),
            symbols: Vec::new(),
        });
    }
    if shstrndx >= SHN_LORESERVE {
        return Err(format!(
            "extended section numbering (shstrndx {shstrndx:#x})"
        ));
    }
    let raw = read_at(
        &f,
        shoff,
        u64::from(shentsize) * u64::from(shnum),
        "section headers",
    )?;
    let mut sections = Vec::with_capacity(shnum as usize);
    let mut names = Vec::with_capacity(shnum as usize);
    for i in 0..shnum as usize {
        let o = i * shentsize as usize;
        // Elf32_Shdr: name type flags addr offset size link info align entsize,
        // all 4 bytes. Elf64_Shdr: name/type 4, flags/addr/offset/size 8,
        // link/info 4, align/entsize 8.
        let (flags, addr, offset, size, link, entsize) = if wide {
            (
                ord.u64(&raw, o + 8)?,
                ord.u64(&raw, o + 16)?,
                ord.u64(&raw, o + 24)?,
                ord.u64(&raw, o + 32)?,
                ord.u32(&raw, o + 40)?,
                ord.u64(&raw, o + 56)?,
            )
        } else {
            (
                ord.u32(&raw, o + 8)?.into(),
                ord.u32(&raw, o + 12)?.into(),
                ord.u32(&raw, o + 16)?.into(),
                ord.u32(&raw, o + 20)?.into(),
                ord.u32(&raw, o + 24)?,
                ord.u32(&raw, o + 36)?.into(),
            )
        };
        names.push(ord.u32(&raw, o)?);
        sections.push(Section {
            name: String::new(),
            kind: ord.u32(&raw, o + 4)?,
            flags,
            addr,
            offset,
            size,
            link,
            entsize,
        });
    }
    let shstr = sections
        .get(shstrndx as usize)
        .ok_or_else(|| format!("shstrndx {shstrndx} out of {shnum}"))?;
    let shstr = read_at(&f, shstr.offset, shstr.size, "section name table")?;
    for (s, n) in sections.iter_mut().zip(names) {
        s.name = c_str(&shstr, n as usize);
    }

    let sym_size = if wide { 24 } else { 16 };
    let mut symbols = Vec::new();
    for symtab in sections.iter().filter(|s| s.kind == SHT_SYMTAB) {
        if symtab.entsize != sym_size {
            return Err(format!(
                "`{}` entsize {} (expected {sym_size})",
                symtab.name, symtab.entsize
            ));
        }
        let strtab = sections
            .get(symtab.link as usize)
            .ok_or_else(|| format!("`{}` links section {}", symtab.name, symtab.link))?;
        let strings = read_at(&f, strtab.offset, strtab.size, "symbol string table")?;
        let table = read_at(&f, symtab.offset, symtab.size, "symbol table")?;
        symbols.reserve(table.len() / sym_size as usize);
        for e in table.chunks_exact(sym_size as usize) {
            // Elf32_Sym: name value size (4 each), info other (1 each), shndx 2.
            // Elf64_Sym: name 4, info other 1 each, shndx 2, value size 8 each.
            let (value, size, info, shndx) = if wide {
                (ord.u64(e, 8)?, ord.u64(e, 16)?, e[4], ord.u16(e, 6)?)
            } else {
                (
                    ord.u32(e, 4)?.into(),
                    ord.u32(e, 8)?.into(),
                    e[12],
                    ord.u16(e, 14)?,
                )
            };
            symbols.push(Symbol {
                name: c_str(&strings, ord.u32(e, 0)? as usize),
                value,
                size,
                bind: info >> 4,
                kind: info & 0xf,
                shndx,
            });
        }
    }
    Ok(Elf { sections, symbols })
}

/// Build a small, valid ELF in memory, for the negative controls of the
/// tests that read symbols. Not a general writer: one `.text` (code), one
/// `.data`, one `.symtab` and its two string tables, in the requested class
/// and byte order.
#[doc(hidden)]
pub mod synth {
    /// `(name, binding, section)` -- section `"text"`, `"data"` or `"undef"`.
    pub type SymSpec<'a> = (&'a str, u8, &'a str);

    pub fn build(wide: bool, big: bool, syms: &[SymSpec<'_>]) -> Vec<u8> {
        let put16 = |v: u16| {
            if big {
                v.to_be_bytes().to_vec()
            } else {
                v.to_le_bytes().to_vec()
            }
        };
        let put32 = |v: u32| {
            if big {
                v.to_be_bytes().to_vec()
            } else {
                v.to_le_bytes().to_vec()
            }
        };
        let putw = |v: u64| {
            if wide {
                if big {
                    v.to_be_bytes().to_vec()
                } else {
                    v.to_le_bytes().to_vec()
                }
            } else {
                put32(v as u32)
            }
        };
        let shstr = b"\0.text\0.data\0.symtab\0.strtab\0.shstrtab\0".to_vec();
        let name_at = |n: &str| {
            let hay = String::from_utf8_lossy(&shstr).into_owned();
            hay.find(&format!("\0{n}\0")).unwrap() as u32 + 1
        };
        let mut strtab = vec![0u8];
        let mut symtab = Vec::new();
        let sym_size = if wide { 24 } else { 16 };
        symtab.extend(vec![0u8; sym_size]); // the null symbol
        for (name, bind, sec) in syms {
            let off = strtab.len() as u32;
            strtab.extend_from_slice(name.as_bytes());
            strtab.push(0);
            let shndx: u16 = match *sec {
                "text" => 1,
                "data" => 2,
                _ => 0,
            };
            let kind: u8 = if *sec == "text" { 2 } else { 1 };
            let info = (bind << 4) | kind;
            if wide {
                symtab.extend(put32(off));
                symtab.push(info);
                symtab.push(0);
                symtab.extend(put16(shndx));
                symtab.extend(putw(0x1000));
                symtab.extend(putw(4));
            } else {
                symtab.extend(put32(off));
                symtab.extend(put32(0x1000));
                symtab.extend(put32(4));
                symtab.push(info);
                symtab.push(0);
                symtab.extend(put16(shndx));
            }
        }
        let ehsize = if wide { 64 } else { 52 };
        let text = vec![0u8; 16];
        let data = vec![0u8; 16];
        // Layout: ehdr | text | data | symtab | strtab | shstrtab | shdrs.
        let off_text = ehsize;
        let off_data = off_text + text.len();
        let off_symtab = off_data + data.len();
        let off_strtab = off_symtab + symtab.len();
        let off_shstr = off_strtab + strtab.len();
        let shoff = off_shstr + shstr.len();
        let shentsize = if wide { 64 } else { 40 };

        let mut out = vec![0x7f, b'E', b'L', b'F', if wide { 2 } else { 1 }];
        out.push(if big { 2 } else { 1 });
        out.push(1);
        out.resize(16, 0);
        out.extend(put16(2)); // e_type EXEC
        out.extend(put16(62)); // e_machine
        out.extend(put32(1)); // e_version
        out.extend(putw(0)); // e_entry
        out.extend(putw(0)); // e_phoff
        out.extend(putw(shoff as u64));
        out.extend(put32(0)); // e_flags
        out.extend(put16(ehsize as u16));
        out.extend(put16(0)); // e_phentsize
        out.extend(put16(0)); // e_phnum
        out.extend(put16(shentsize));
        out.extend(put16(6)); // e_shnum
        out.extend(put16(5)); // e_shstrndx
        assert_eq!(out.len(), ehsize);
        out.extend(&text);
        out.extend(&data);
        out.extend(&symtab);
        out.extend(&strtab);
        out.extend(&shstr);
        // (name, type, flags, offset, size, link, entsize)
        let shdrs: [(u32, u32, u64, usize, usize, u32, u64); 6] = [
            (0, 0, 0, 0, 0, 0, 0),
            (name_at(".text"), 1, 0x6, off_text, text.len(), 0, 0),
            (name_at(".data"), 1, 0x3, off_data, data.len(), 0, 0),
            (
                name_at(".symtab"),
                2,
                0,
                off_symtab,
                symtab.len(),
                4,
                sym_size as u64,
            ),
            (name_at(".strtab"), 3, 0, off_strtab, strtab.len(), 0, 0),
            (name_at(".shstrtab"), 3, 0, off_shstr, shstr.len(), 0, 0),
        ];
        for (name, kind, flags, offset, size, link, entsize) in shdrs {
            out.extend(put32(name));
            out.extend(put32(kind));
            out.extend(putw(flags));
            out.extend(putw(0)); // addr
            out.extend(putw(offset as u64));
            out.extend(putw(size as u64));
            out.extend(put32(link));
            out.extend(put32(0)); // info
            out.extend(putw(1)); // addralign
            out.extend(putw(entsize));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_bytes(bytes: &[u8]) -> Result<Elf, String> {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.elf");
        std::fs::write(&p, bytes).unwrap();
        read(&p)
    }

    /// Every class and byte order reads back the same table, and `is_text`
    /// keeps exactly `nm`'s `T`/`t`: not the import, not data, not weak.
    #[test]
    fn reads_both_classes_and_byte_orders() {
        let syms: &[synth::SymSpec<'_>] = &[
            ("global_fn", 1, "text"),
            ("local_fn", 0, "text"),
            ("weak_fn", STB_WEAK, "text"),
            ("a_datum", 1, "data"),
            ("an_import", 1, "undef"),
        ];
        for wide in [false, true] {
            for big in [false, true] {
                let elf = read_bytes(&synth::build(wide, big, syms))
                    .unwrap_or_else(|e| panic!("wide={wide} big={big}: {e}"));
                let text: Vec<&str> = elf
                    .symbols
                    .iter()
                    .filter(|s| elf.is_text(s))
                    .map(|s| s.name.as_str())
                    .collect();
                assert_eq!(text, ["global_fn", "local_fn"], "wide={wide} big={big}");
                assert_eq!(elf.sections[1].name, ".text");
                assert!(elf.defined("an_import").is_none());
                assert!(elf.defined("a_datum").is_some());
                assert_eq!(elf.defined("global_fn").unwrap().value, 0x1000);
            }
        }
    }

    #[test]
    fn refuses_what_is_not_an_elf() {
        assert!(read_bytes(b"#!/bin/sh\necho hi\n").is_err());
        assert!(read_bytes(b"\x7fEL").is_err());
        let mut truncated = synth::build(true, false, &[("f", 1, "text")]);
        truncated.truncate(80);
        assert!(read_bytes(&truncated).is_err());
    }
}
