//! phase-578 W2 (RFC-0118 D9): a fault's addresses as source lines.
//!
//! The console of every target verb (`nros run`, `nros monitor`,
//! `nros flash --monitor`) passes through [`FaultLines`]. When a line is a
//! fault report it recognises, the addresses in it are looked up in the
//! image's ELF, in this process (the `addr2line` crate, so no
//! `<triple>-addr2line` is needed), and the function, file and line are
//! printed under it:
//!
//! ```text
//! nros-fault: kind=hardfault pc=0x00001a2c lr=0x00001b07 sp=0x2000ff40 ...
//! nros: symbolizing with build/freertos/app.elf
//! nros:   pc 0x00001a2c: fault_inject at src/main.c:42
//! nros:   lr 0x00001b07: app_main at src/main.c:80
//! ```
//!
//! The formats recognised (RFC-0118 D9, D2 of the 2026-11-26 ruling):
//!
//! * the `nros-fault:` line every nano-ros-owned fault handler prints
//!   (`key=value`; `pc` and `lr` are looked up, the rest is context);
//! * Zephyr's own fatal output, which nano-ros does not own:
//!   `Faulting instruction address (r15/pc): 0x…` and the `r14/lr:` field of
//!   its register dump;
//! * `CONFIG_NROS_FAULT_LOG`'s record (`nros:   pc     : 0x…`).
//!
//! A Rust panic line already names its file and line, so it is left as it is.
//!
//! A wrong ELF gives confident wrong lines, so the ELF used is named once,
//! before the first lookup, and an address outside the ELF's executable
//! sections is reported as such rather than matched to the nearest symbol.

use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use addr2line::{
    Context, LookupResult,
    gimli::{self, EndianSlice, RunTimeEndian},
};
use object::{Object, ObjectSection, ObjectSymbol, SectionFlags, SectionKind};

/// Which register an address came from, as the report names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reg {
    /// The faulting instruction.
    Pc,
    /// The link register: the return address in the caller.
    Lr,
}

impl Reg {
    fn word(self) -> &'static str {
        match self {
            Self::Pc => "pc",
            Self::Lr => "lr",
        }
    }
}

/// The addresses one console line reports, in the order they are printed.
pub type Addresses = Vec<(Reg, u64)>;

fn hex(s: &str) -> Option<u64> {
    let s = s.trim();
    let digits = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))?;
    let digits: String = digits.chars().take_while(char::is_ascii_hexdigit).collect();
    if digits.is_empty() {
        return None;
    }
    u64::from_str_radix(&digits, 16).ok()
}

/// The value after `label` in `line`: `<label>` then spaces or `:`, then a
/// `0x` number.
fn after(line: &str, label: &str) -> Option<u64> {
    let at = line.find(label)? + label.len();
    let rest = line[at..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    hex(rest)
}

/// The addresses `line` reports, if it is a fault report this module
/// recognises. `None` for every other line, and for a panic line, which needs
/// no lookup.
#[must_use]
pub fn parse(line: &str) -> Option<Addresses> {
    let line = line.trim_end_matches(['\r', '\n']);
    // The nano-ros line: `nros-fault: kind=… pc=0x… lr=0x… …`.
    if let Some(at) = line.find("nros-fault:") {
        let mut out = Addresses::new();
        for field in line[at + "nros-fault:".len()..].split_whitespace() {
            let Some((k, v)) = field.split_once('=') else {
                continue;
            };
            let reg = match k {
                "pc" => Reg::Pc,
                "lr" => Reg::Lr,
                _ => continue,
            };
            if let Some(a) = hex(v) {
                out.push((reg, a));
            }
        }
        return (!out.is_empty()).then_some(out);
    }
    // Zephyr's fatal output.
    if line.contains("Faulting instruction address (r15/pc)") {
        return after(line, "(r15/pc)").map(|a| vec![(Reg::Pc, a)]);
    }
    if line.contains("r14/lr:") {
        return after(line, "r14/lr:").map(|a| vec![(Reg::Lr, a)]);
    }
    // `CONFIG_NROS_FAULT_LOG`'s record: `nros:   pc     : 0x…`.
    if let Some(rest) = line.trim_start().strip_prefix("nros:") {
        let rest = rest.trim_start();
        for (word, reg) in [("pc", Reg::Pc), ("lr", Reg::Lr)] {
            if let Some(v) = rest.strip_prefix(word)
                && v.trim_start().starts_with(':')
            {
                return after(v, "").map(|a| vec![(reg, a)]);
            }
        }
    }
    None
}

/// One frame of a lookup: the function and where in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The function, demangled where it is Rust; `None` when nothing names it.
    pub function: Option<String>,
    /// `file:line`, when the ELF has line information for the address.
    pub location: Option<String>,
}

/// What a lookup found for one address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Lookup {
    /// The innermost frame first, then each inlined-into caller.
    Frames(Vec<Frame>),
    /// The address is in none of the ELF's executable sections.
    NotInElf,
}

/// An ELF opened for lookups.
pub struct Symbolizer {
    path: PathBuf,
    ctx: Context<EndianSlice<'static, RunTimeEndian>>,
    /// The executable sections' address ranges.
    text: Vec<(u64, u64)>,
    /// The symbol table's functions, `(address, size, name)`, sorted: names a
    /// function the DWARF does not describe (an image built without `-g`).
    symbols: Vec<(u64, u64, String)>,
    /// Whether the image is Thumb-capable Arm, where bit 0 of a code address
    /// is the Thumb state, not part of the address.
    arm: bool,
}

impl Symbolizer {
    /// Open `elf`.
    ///
    /// # Errors
    ///
    /// The file cannot be read or is not an object file, or its debug
    /// sections are compressed (no decompressor is linked).
    pub fn open(elf: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(elf).map_err(|e| format!("reading `{}`: {e}", elf.display()))?;
        // One ELF per console session; its sections are borrowed for the
        // session's life, so the bytes are kept for the process's.
        let bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
        let file = object::File::parse(bytes)
            .map_err(|e| format!("`{}` is not an object file: {e}", elf.display()))?;
        let endian = if file.is_little_endian() {
            RunTimeEndian::Little
        } else {
            RunTimeEndian::Big
        };
        let mut compressed = None;
        let load = |id: gimli::SectionId| -> Result<EndianSlice<'static, RunTimeEndian>, ()> {
            let data = match file.section_by_name(id.name()) {
                Some(s) => {
                    if s.compressed_file_range()
                        .is_ok_and(|r| r.format != object::CompressionFormat::None)
                    {
                        compressed.get_or_insert(id.name());
                        &[][..]
                    } else {
                        s.data().unwrap_or(&[])
                    }
                }
                None => &[][..],
            };
            Ok(EndianSlice::new(data, endian))
        };
        let dwarf = gimli::Dwarf::load(load).map_err(|()| "loading DWARF".to_string())?;
        if let Some(name) = compressed {
            return Err(format!(
                "`{}`'s `{name}` is compressed, and this `nros` reads only uncompressed \
                 debug sections",
                elf.display()
            ));
        }
        let ctx = Context::from_dwarf(dwarf)
            .map_err(|e| format!("reading `{}`'s DWARF: {e}", elf.display()))?;
        let text = file
            .sections()
            .filter(|s| {
                s.kind() == SectionKind::Text
                    || matches!(s.flags(), SectionFlags::Elf { sh_flags }
                        if sh_flags & u64::from(object::elf::SHF_EXECINSTR) != 0)
            })
            .filter(|s| s.size() > 0)
            .map(|s| (s.address(), s.address() + s.size()))
            .collect();
        let mut symbols: Vec<(u64, u64, String)> = file
            .symbols()
            .filter(|s| s.kind() == object::SymbolKind::Text)
            .filter_map(|s| Some((s.address(), s.size(), s.name().ok()?.to_string())))
            .collect();
        symbols.sort();
        Ok(Self {
            path: elf.to_path_buf(),
            ctx,
            text,
            symbols,
            arm: file.architecture() == object::Architecture::Arm,
        })
    }

    /// The ELF this symbolizer reads.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Look up `addr` as `reg` reported it. A return address (`lr`) is looked
    /// up one byte back, inside the call instruction, so it names the call
    /// site rather than the line after it.
    #[must_use]
    pub fn lookup(&self, reg: Reg, addr: u64) -> Lookup {
        let mut probe = if self.arm { addr & !1 } else { addr };
        if reg == Reg::Lr {
            probe = probe.saturating_sub(1);
        }
        if !self.text.iter().any(|(lo, hi)| (*lo..*hi).contains(&probe)) {
            return Lookup::NotInElf;
        }
        let mut frames = Vec::new();
        if let LookupResult::Output(Ok(mut iter)) = self.ctx.find_frames(probe) {
            while let Ok(Some(f)) = iter.next() {
                let function = f
                    .function
                    .as_ref()
                    .and_then(|n| n.demangle().ok())
                    .map(Cow::into_owned);
                let location = f.location.as_ref().and_then(|l| {
                    let file = l.file?;
                    Some(match l.line {
                        Some(line) => format!("{file}:{line}"),
                        None => file.to_string(),
                    })
                });
                frames.push(Frame { function, location });
            }
        }
        // No DWARF for this address (an image built without `-g`): the symbol
        // table still names the function.
        if frames.iter().all(|f| f.function.is_none()) {
            let named = self.symbol_at(probe);
            match frames.first_mut() {
                Some(first) => first.function = named,
                None => frames.push(Frame {
                    function: named,
                    location: None,
                }),
            }
        }
        Lookup::Frames(frames)
    }

    fn symbol_at(&self, probe: u64) -> Option<String> {
        let probe_sym = |a: u64| if self.arm { a & !1 } else { a };
        self.symbols
            .iter()
            .filter(|(a, size, _)| {
                let a = probe_sym(*a);
                probe >= a && (probe < a + (*size).max(1))
            })
            .max_by_key(|(a, ..)| *a)
            .map(|(.., n)| addr2line::demangle_auto(Cow::Borrowed(n), None).into_owned())
    }
}

/// Render a lookup as the lines printed under the fault line.
#[must_use]
pub fn render(reg: Reg, addr: u64, lookup: &Lookup, elf: &Path) -> Vec<String> {
    let head = format!("{} {addr:#010x}", reg.word());
    match lookup {
        Lookup::NotInElf => vec![format!(
            "nros:   {head}: not in `{}`'s code; is it the image that ran?",
            elf.display()
        )],
        Lookup::Frames(frames) => {
            let one = |f: &Frame| {
                let name = f.function.as_deref().unwrap_or("??");
                match &f.location {
                    Some(at) => format!("{name} at {at}"),
                    None => name.to_string(),
                }
            };
            let mut out = Vec::new();
            for (i, f) in frames.iter().enumerate() {
                if i == 0 {
                    out.push(format!("nros:   {head}: {}", one(f)));
                } else {
                    out.push(format!("nros:     inlined into {}", one(f)));
                }
            }
            out
        }
    }
}

/// Where the ELF for lookups comes from (RFC-0118 D9, D4 of the ruling).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ElfSource {
    /// `--elf <path>`.
    Flag(PathBuf),
    /// The run plan's `elf` artifact, for the image the verb selected.
    Plan(PathBuf),
    /// None: a fault is printed as it came, with one note on how to name it.
    None,
}

impl ElfSource {
    /// The ladder: the flag, then the plan's artifact.
    #[must_use]
    pub fn resolve(flag: Option<&Path>, plan_elf: Option<&Path>) -> Self {
        match (flag, plan_elf) {
            (Some(f), _) => Self::Flag(f.to_path_buf()),
            (None, Some(p)) => Self::Plan(p.to_path_buf()),
            (None, None) => Self::None,
        }
    }

    fn path(&self) -> Option<&Path> {
        match self {
            Self::Flag(p) | Self::Plan(p) => Some(p),
            Self::None => None,
        }
    }
}

/// The console's fault watch: fed every chunk the console prints, it returns
/// the lines to print after it. The ELF is opened at the first fault, never
/// before, so a session with no fault reads nothing.
pub struct FaultLines {
    source: ElfSource,
    line: Vec<u8>,
    opened: Option<Result<Symbolizer, String>>,
    noted: bool,
}

impl FaultLines {
    /// A watch that looks addresses up in `source`'s ELF.
    #[must_use]
    pub fn new(source: ElfSource) -> Self {
        Self {
            source,
            line: Vec::new(),
            opened: None,
            noted: false,
        }
    }

    /// Feed a chunk of console output; returns the lines to print after it,
    /// each ending in `\n`.
    pub fn feed(&mut self, chunk: &[u8]) -> String {
        let mut out = String::new();
        for &b in chunk {
            if b == b'\n' {
                let line = String::from_utf8_lossy(&self.line).into_owned();
                self.line.clear();
                for l in self.on_line(&line) {
                    out.push_str(&l);
                    out.push('\n');
                }
            } else if self.line.len() < 4096 {
                self.line.push(b);
            }
        }
        out
    }

    /// The lines to print under one complete console line.
    pub fn on_line(&mut self, line: &str) -> Vec<String> {
        let Some(addrs) = parse(line) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let Some(elf) = self.source.path().map(Path::to_path_buf) else {
            if !self.noted {
                self.noted = true;
                out.push(
                    "nros: that is a fault report; pass `--elf <the image's ELF>` to see its \
                     source lines"
                        .to_string(),
                );
            }
            return out;
        };
        let first = self.opened.is_none();
        let opened = self.opened.get_or_insert_with(|| Symbolizer::open(&elf));
        match opened {
            Err(e) => {
                if !self.noted {
                    self.noted = true;
                    out.push(format!("nros: cannot look the fault up: {e}"));
                }
            }
            Ok(sym) => {
                if first {
                    out.push(format!("nros: symbolizing with {}", sym.path().display()));
                }
                for (reg, addr) in addrs {
                    out.extend(render(reg, addr, &sym.lookup(reg, addr), sym.path()));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nano_ros_line_gives_pc_and_lr() {
        let l = "nros-fault: kind=hardfault pc=0x00001a2c lr=0x00001b07 sp=0x2000ff40 \
                 psr=0x61000000 cfsr=0x00008200 task=app";
        assert_eq!(parse(l), Some(vec![(Reg::Pc, 0x1a2c), (Reg::Lr, 0x1b07)]));
        // Anywhere in the line, as a console prefix may put it.
        assert_eq!(
            parse("[00:00:01.000] nros-fault: kind=abort pc=0x10 dfar=0x0"),
            Some(vec![(Reg::Pc, 0x10)])
        );
        assert_eq!(parse("nros-fault: kind=hardfault"), None);
    }

    #[test]
    fn zephyrs_fatal_output_gives_pc_and_lr() {
        assert_eq!(
            parse("E: Faulting instruction address (r15/pc): 0x000012f4"),
            Some(vec![(Reg::Pc, 0x12f4)])
        );
        assert_eq!(
            parse("E: r12/ip:  0x00000000 r14/lr:  0x00001321"),
            Some(vec![(Reg::Lr, 0x1321)])
        );
    }

    #[test]
    fn the_fault_log_record_gives_pc_and_lr() {
        assert_eq!(
            parse("nros:   pc     : 0x0000abcd"),
            Some(vec![(Reg::Pc, 0xabcd)])
        );
        assert_eq!(
            parse("nros:   lr     : 0x0000abc1"),
            Some(vec![(Reg::Lr, 0xabc1)])
        );
        assert_eq!(parse("nros:   psr    : 0x61000000"), None);
    }

    #[test]
    fn other_lines_and_panics_are_left_alone() {
        for l in [
            "Publishing: 3",
            "panicked at src/main.rs:10:5:",
            "nros: thread main (0x2000) died holding the nros_log dispatcher",
            "pc=0x1234 without the marker",
        ] {
            assert_eq!(parse(l), None, "{l}");
        }
    }

    #[test]
    fn the_elf_ladder_is_flag_then_plan() {
        let f = Path::new("/f.elf");
        let p = Path::new("/p.elf");
        assert_eq!(
            ElfSource::resolve(Some(f), Some(p)),
            ElfSource::Flag(f.into())
        );
        assert_eq!(ElfSource::resolve(None, Some(p)), ElfSource::Plan(p.into()));
        assert_eq!(ElfSource::resolve(None, None), ElfSource::None);
    }

    #[test]
    fn with_no_elf_a_fault_gets_one_note() {
        let mut w = FaultLines::new(ElfSource::None);
        let out = w.feed(b"boot\nnros-fault: kind=hardfault pc=0x10\nnros-fault: pc=0x20\n");
        assert_eq!(out.matches("--elf").count(), 1, "{out}");
    }

    #[test]
    fn an_unreadable_elf_is_said_once() {
        let mut w = FaultLines::new(ElfSource::Flag("/nonexistent/app.elf".into()));
        let out = w.feed(b"nros-fault: pc=0x10\nnros-fault: pc=0x20\n");
        assert_eq!(out.matches("cannot look the fault up").count(), 1, "{out}");
    }

    /// The committed fixture (`testdata/fault-lines/fixture.c` and its
    /// `fixture.elf`, Cortex-M3 Thumb, built with `-g`): the tests do not
    /// depend on the host build's debug info, which CI turns off.
    fn fixture() -> (Symbolizer, String) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fault-lines");
        let src = std::fs::read_to_string(dir.join("fixture.c")).unwrap();
        (Symbolizer::open(&dir.join("fixture.elf")).unwrap(), src)
    }

    /// The 1-based line of `src` holding `marker`.
    fn line_of(src: &str, marker: &str) -> u32 {
        let at = src.lines().position(|l| l.contains(marker)).unwrap();
        u32::try_from(at + 1).unwrap()
    }

    /// The fixture's symbol `name`, as the symbol table states it (bit 0 set:
    /// a Thumb function).
    fn symbol(name: &str) -> u64 {
        let bytes = std::fs::read(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fault-lines/fixture.elf"),
        )
        .unwrap();
        let file = object::File::parse(&*bytes).unwrap();
        file.symbols()
            .find(|s| s.name() == Ok(name))
            .map(|s| s.address())
            .unwrap()
    }

    #[test]
    fn a_pc_resolves_to_its_function_and_line() {
        let (sym, src) = fixture();
        let want = line_of(&src, "fault-lines: inner store");
        // The function's first instruction is the store; the symbol carries
        // the Thumb bit, which the lookup clears.
        let pc = symbol("fixture_inner");
        assert_eq!(pc & 1, 1, "a Thumb symbol");
        let got = sym.lookup(Reg::Pc, pc);
        assert_eq!(
            got,
            Lookup::Frames(vec![Frame {
                function: Some("fixture_inner".into()),
                location: Some(format!("./fixture.c:{want}")),
            }])
        );
    }

    /// A return address names the call, with the inlined function the call
    /// sits in and the function that inlined it.
    #[test]
    fn an_lr_resolves_to_its_call_site_and_inlined_callers() {
        let (sym, src) = fixture();
        let call = line_of(&src, "fault-lines: inlined call");
        let site = line_of(&src, "fixture_middle(&x);");
        // `_start`'s `bl fixture_inner` is 4 bytes at +0xa; the return
        // address is the next instruction, with the Thumb bit.
        let lr = (symbol("_start") & !1) + 0xe + 1;
        match sym.lookup(Reg::Lr, lr) {
            Lookup::Frames(frames) => {
                assert_eq!(frames.len(), 2, "{frames:?}");
                assert_eq!(frames[0].function.as_deref(), Some("fixture_middle"));
                assert_eq!(
                    frames[0].location.as_deref(),
                    Some(format!("./fixture.c:{call}").as_str())
                );
                assert_eq!(frames[1].function.as_deref(), Some("_start"));
                assert_eq!(
                    frames[1].location.as_deref(),
                    Some(format!("./fixture.c:{site}").as_str())
                );
                let lines = render(Reg::Lr, lr, &Lookup::Frames(frames), sym.path());
                assert!(
                    lines[0].contains(": fixture_middle at ./fixture.c:"),
                    "{lines:?}"
                );
                assert!(lines[1].contains("inlined into _start at"), "{lines:?}");
            }
            Lookup::NotInElf => panic!("the call is in the fixture's code"),
        }
    }

    #[test]
    fn an_address_outside_the_code_is_refused() {
        let (sym, _) = fixture();
        assert_eq!(sym.lookup(Reg::Pc, 0x0010_0000), Lookup::NotInElf);
        let lines = render(Reg::Pc, 0x0010_0000, &Lookup::NotInElf, sym.path());
        assert!(lines[0].contains("not in"), "{lines:?}");
    }

    /// End to end through the console watch: the ELF is named once, then the
    /// lookup.
    #[test]
    fn the_watch_names_the_elf_once_then_the_lines() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/fault-lines");
        let mut w = FaultLines::new(ElfSource::Flag(dir.join("fixture.elf")));
        let pc = symbol("fixture_inner");
        let out = w.feed(
            format!("nros-fault: kind=hardfault pc={pc:#010x}\nnros-fault: pc={pc:#010x}\n")
                .as_bytes(),
        );
        assert_eq!(out.matches("symbolizing with").count(), 1, "{out}");
        assert_eq!(
            out.matches(": fixture_inner at ./fixture.c:").count(),
            2,
            "{out}"
        );
    }
}
