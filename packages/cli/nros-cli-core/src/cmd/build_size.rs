//! What a built image occupies, printed after every `nros build` (phase-578 W1).
//!
//! Read from the linked ELF's section headers, by the split the GNU `size`
//! tool's Berkeley format uses, so the numbers can be checked against it:
//!
//! - **text**: allocated, not writable (code, read-only data, vectors);
//! - **data**: allocated, writable, with file contents (initialised data:
//!   held in flash, copied to RAM at boot);
//! - **bss**: allocated, writable, no file contents (zeroed at boot): an
//!   `SHT_NOBITS` section, which is the one kind with no range in the file.
//!
//! Flash used is `text + data`; RAM used is `data + bss`. A RAM figure from the
//! sections is the STATIC RAM: the heap and the stacks the image reserves are
//! sections too (a `.heap` or `.noinit` region) when the link places them, and
//! anything the program takes at run time beyond that is not here.
//!
//! The board's capacity is a percentage's denominator. A board definition
//! does not state one yet (its memory budget is RFC-0135 D8, phase-577 W6), so
//! a build prints absolute numbers; [`render`] takes the budget once one
//! exists. A hosted program (an ELF with an interpreter, such as a POSIX
//! image or Zephyr's `native_sim`) has no flash or fixed RAM, so it gets no
//! report.

use std::path::Path;

use object::{Object, ObjectSection, SectionFlags};

const SHF_WRITE: u64 = 0x1;
const SHF_ALLOC: u64 = 0x2;

/// An image's section sizes, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImageSize {
    pub text: u64,
    pub data: u64,
    pub bss: u64,
}

impl ImageSize {
    /// Bytes the image stores in flash: its code and constants, plus the
    /// initial values of its data.
    #[must_use]
    pub fn flash(&self) -> u64 {
        self.text + self.data
    }

    /// Bytes of RAM the image places at link time.
    #[must_use]
    pub fn ram(&self) -> u64 {
        self.data + self.bss
    }
}

/// A board's flash and RAM capacity, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub flash: u64,
    pub ram: u64,
}

/// The sizes of the ELF in `bytes`, or `None` for a hosted program, which has
/// no flash and no fixed RAM.
///
/// # Errors
///
/// `bytes` is not an ELF file `object` can read.
pub fn of_elf(bytes: &[u8]) -> Result<Option<ImageSize>, String> {
    let file = object::File::parse(bytes).map_err(|e| format!("not an ELF image: {e}"))?;
    if file.format() != object::BinaryFormat::Elf {
        return Err(format!("not an ELF image ({:?})", file.format()));
    }
    if file.section_by_name(".interp").is_some() {
        return Ok(None);
    }
    let mut size = ImageSize::default();
    for section in file.sections() {
        let SectionFlags::Elf { sh_flags } = section.flags() else {
            continue;
        };
        if sh_flags & SHF_ALLOC == 0 {
            continue;
        }
        let bytes = section.size();
        if sh_flags & SHF_WRITE == 0 {
            size.text += bytes;
        } else if section.file_range().is_none() {
            size.bss += bytes;
        } else {
            size.data += bytes;
        }
    }
    Ok(Some(size))
}

/// The sizes of the image at `elf`, or `None` for a hosted program.
///
/// # Errors
///
/// The file cannot be read or is not an ELF image.
pub fn of_path(elf: &Path) -> Result<Option<ImageSize>, String> {
    let bytes = std::fs::read(elf).map_err(|e| format!("{}: {e}", elf.display()))?;
    of_elf(&bytes).map_err(|e| format!("{}: {e}", elf.display()))
}

/// `n` bytes as people read them: bytes below 10 KiB, then KiB with one
/// decimal. The byte count is always printed too, so nothing is lost to
/// rounding.
fn human(n: u64) -> String {
    if n < 10 * 1024 {
        format!("{n} B")
    } else {
        format!("{:.1} KiB", n as f64 / 1024.0)
    }
}

/// One line of the report: `used`, and against `capacity` when there is one.
fn line(label: &str, used: u64, capacity: Option<u64>) -> String {
    let mut s = format!("  {label:<6}{:>11}  ({used} B)", human(used));
    if let Some(cap) = capacity.filter(|c| *c > 0) {
        let pct = used as f64 * 100.0 / cap as f64;
        s.push_str(&format!(" of {} ({pct:.1} %)", human(cap)));
        if used > cap {
            s.push_str(&format!(", {} over", human(used - cap)));
        }
    }
    s.push('\n');
    s
}

/// The report `nros build` prints for one image.
#[must_use]
pub fn render(qualified: &str, size: &ImageSize, budget: Option<&Budget>) -> String {
    let mut s = format!("nros build: size of `{qualified}`:\n");
    s.push_str(&line("flash", size.flash(), budget.map(|b| b.flash)));
    s.push_str(&line("RAM", size.ram(), budget.map(|b| b.ram)));
    s.push_str(&format!(
        "  (text {} + data {} in flash; data {} + bss {} in RAM, at link time)\n",
        size.text, size.data, size.data, size.bss
    ));
    if budget.is_none() {
        s.push_str("  the board states no memory budget, so there is no percentage\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One section of a hand-built ELF: name, type, flags, size.
    struct Sec(&'static str, u32, u32, u32);

    const SHT_PROGBITS: u32 = 1;
    const SHT_NOBITS: u32 = 8;
    const SHT_STRTAB: u32 = 3;
    const SHF_EXECINSTR: u32 = 0x4;

    /// A minimal 32-bit little-endian ARM ELF executable with `secs` (after
    /// the null section and `.shstrtab`). A NOBITS section stores no bytes;
    /// the others store `size` zero bytes, so the file is what a linker
    /// writes for these headers.
    fn elf32(secs: &[Sec]) -> Vec<u8> {
        let mut names = vec![0u8];
        let mut name_off = Vec::new();
        for s in secs.iter().map(|s| s.0).chain([".shstrtab"]) {
            name_off.push(names.len() as u32);
            names.extend_from_slice(s.as_bytes());
            names.push(0);
        }
        let mut body = Vec::new();
        let mut offs = Vec::new();
        for s in secs {
            offs.push(52 + body.len() as u32);
            if s.1 != SHT_NOBITS {
                body.resize(body.len() + s.3 as usize, 0);
            }
        }
        let strtab_off = 52 + body.len() as u32;
        body.extend_from_slice(&names);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        let shoff = 52 + body.len() as u32;
        let shnum = secs.len() as u16 + 2;

        let mut f = Vec::new();
        f.extend_from_slice(&[0x7f, b'E', b'L', b'F', 1, 1, 1, 0]);
        f.extend_from_slice(&[0; 8]);
        f.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        f.extend_from_slice(&40u16.to_le_bytes()); // EM_ARM
        f.extend_from_slice(&1u32.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes()); // entry
        f.extend_from_slice(&0u32.to_le_bytes()); // phoff
        f.extend_from_slice(&shoff.to_le_bytes());
        f.extend_from_slice(&0u32.to_le_bytes()); // flags
        f.extend_from_slice(&52u16.to_le_bytes());
        f.extend_from_slice(&0u16.to_le_bytes()); // phentsize
        f.extend_from_slice(&0u16.to_le_bytes()); // phnum
        f.extend_from_slice(&40u16.to_le_bytes());
        f.extend_from_slice(&shnum.to_le_bytes());
        f.extend_from_slice(&(shnum - 1).to_le_bytes()); // shstrndx
        assert_eq!(f.len(), 52);
        f.extend_from_slice(&body);
        let mut sh = |name: u32, ty: u32, flags: u32, off: u32, size: u32| {
            for w in [name, ty, flags, 0, off, size, 0, 0, 1, 0] {
                f.extend_from_slice(&w.to_le_bytes());
            }
        };
        sh(0, 0, 0, 0, 0);
        for (i, s) in secs.iter().enumerate() {
            sh(name_off[i], s.1, s.2, offs[i], s.3);
        }
        sh(
            name_off[secs.len()],
            SHT_STRTAB,
            0,
            strtab_off,
            names.len() as u32,
        );
        f
    }

    const A: u32 = SHF_ALLOC as u32;
    const W: u32 = SHF_WRITE as u32;

    fn mcu_image() -> Vec<u8> {
        elf32(&[
            Sec(".isr_vector", SHT_PROGBITS, A, 0x400),
            Sec(".text", SHT_PROGBITS, A | SHF_EXECINSTR, 12_000),
            Sec(".rodata", SHT_PROGBITS, A, 3_000),
            Sec(".data", SHT_PROGBITS, A | W, 500),
            Sec(".bss", SHT_NOBITS, A | W, 20_000),
            Sec(".noinit", SHT_NOBITS, A | W, 4_096),
            // Not allocated: never counted, whatever their size.
            Sec(".comment", SHT_PROGBITS, 0, 64),
            Sec(".debug_info", SHT_PROGBITS, 0, 9_999),
        ])
    }

    /// The acceptance's "the numbers equal the ELF's": every allocated
    /// section is counted once, in its Berkeley column, and nothing else is.
    #[test]
    fn the_sizes_are_the_elf_sections_by_the_berkeley_split() {
        let s = of_elf(&mcu_image())
            .unwrap()
            .expect("an MCU image is reported");
        assert_eq!(s.text, 0x400 + 12_000 + 3_000);
        assert_eq!(s.data, 500);
        assert_eq!(s.bss, 20_000 + 4_096);
        assert_eq!(s.flash(), 0x400 + 12_000 + 3_000 + 500);
        assert_eq!(s.ram(), 500 + 20_000 + 4_096);
    }

    #[test]
    fn a_hosted_program_gets_no_report() {
        let hosted = elf32(&[
            Sec(".interp", SHT_PROGBITS, A, 20),
            Sec(".text", SHT_PROGBITS, A | SHF_EXECINSTR, 100),
        ]);
        assert_eq!(of_elf(&hosted).unwrap(), None);
    }

    #[test]
    fn a_file_that_is_not_an_elf_is_refused() {
        assert!(of_elf(b"not an image").is_err());
    }

    #[test]
    fn with_a_budget_the_report_gives_percentages() {
        let s = of_elf(&mcu_image()).unwrap().unwrap();
        let b = Budget {
            flash: 64 * 1024,
            ram: 16 * 1024,
        };
        let r = render("demo:fw", &s, Some(&b));
        // flash 16,524 of 65,536 = 25.2 %; RAM 24,596 of 16,384, over.
        assert!(r.contains("(16524 B) of 64.0 KiB (25.2 %)"), "{r}");
        assert!(
            r.contains("(24596 B) of 16.0 KiB (150.1 %), 8212 B over"),
            "{r}"
        );
        assert!(!r.contains("no memory budget"), "{r}");
    }

    #[test]
    fn without_a_budget_the_report_says_why_there_is_no_percentage() {
        let s = of_elf(&mcu_image()).unwrap().unwrap();
        let r = render("demo:fw", &s, None);
        assert!(r.contains("flash"), "{r}");
        assert!(r.contains("(16524 B)\n"), "{r}");
        assert!(r.contains("(24596 B)\n"), "{r}");
        assert!(r.contains("states no memory budget"), "{r}");
    }
}
