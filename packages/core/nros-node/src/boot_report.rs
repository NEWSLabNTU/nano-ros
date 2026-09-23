//! A fixed RAM record the image writes about itself, for targets where no log
//! sink reaches a human.
//!
//! # Why this exists
//!
//! phase-412 derived six pool counts and the executor arena for the
//! mr-canhubk344 island, and then could not tell whether the derived image was
//! correct. The only available signal was the ROS graph's node count, and one
//! unchanged configuration produced 4, 0, 0, 4, 4 across five runs. Every other
//! channel was already disqualified for that board:
//!
//! * The console is on `lpuart0`, which is not wired on the MR-CANHUBK344.
//!   `lpuart2` is the zenoh serial transport and cannot carry a second protocol.
//! * `nros_log` therefore reaches nothing. Both arena diagnostics (issue 0900)
//!   go through it, so the two messages written specifically to explain an
//!   arena failure are invisible on the one board that needed them.
//! * SEGGER RTT was tried and could not discriminate: a working image and a
//!   derived image both emitted only the Zephyr banner, and a deliberate
//!   positive control produced nothing at all.
//! * Semihosting halts the core until a probe answers and FAULTS with no probe
//!   attached, so an image carrying it cannot run standalone.
//!
//! What all of those share is that they are STREAMS: they need the board to
//! still be running, and they need somebody attached at the moment the
//! interesting thing happens. The failure this campaign is trying to see is the
//! opposite shape. An under-sized arena halts DURING entity creation, before
//! the first spin, so issue 0900's advisory never prints -- the failure cannot
//! report itself through any stream.
//!
//! So this is not a stream. It is a fixed-size record in RAM that the image
//! keeps up to date as it boots, read out AFTERWARDS by halting the core and
//! dumping memory. It survives the halt because it does not depend on anything
//! still running, and a PARTIAL record is the useful case rather than a lost
//! one: the last stage reached and the allocation that did not fit are exactly
//! what names the knob to change.
//!
//! # Reading it
//!
//! The record is a `#[no_mangle]` static, so it has a symbol in the ELF and a
//! debugger can find it without the address being wired in anywhere:
//!
//! ```text
//! pyocd commander -t s32k344 -c "halt" -c "savemem <addr> <len> report.bin"
//! python3 scripts/read-boot-report.py <elf> report.bin
//! ```
//!
//! [`MAGIC`] distinguishes a written record from uninitialised RAM, and
//! [`BootReport::struct_size`] lets a reader refuse a layout it does not know
//! rather than decode it wrongly.
//!
//! # Cost, and why it is opt-in
//!
//! Enabled by setting `NROS_BOOT_REPORT=1` at build time (Zephyr:
//! `CONFIG_NROS_BOOT_REPORT=y`), which makes `nros-node`'s build script emit
//! `cfg(nros_boot_report)`. With the cfg absent every function here is an empty
//! `#[inline(always)]` body and the static does not exist, so an image that
//! does not opt in is byte-identical to one built before this module -- the
//! same rule issue 0900's arena knob and phase-403's `rx_buffer_from_type()`
//! both keep.
//!
//! Enabled, it costs [`BootReport::struct_size`] bytes of `.bss` -- 92, the
//! same on every target because every field is a `u32` -- and a handful of
//! relaxed atomic stores on paths that run once per entity at registration.
//!
//! The 92 is not a detail: it is the LENGTH an operator types into `savemem`,
//! and this sentence said 60 for as long as the record had fifteen fields. A
//! short dump decodes -- `read-boot-report.py` needs `23 * 4` bytes and an
//! 88-byte one is refused, but a reader who trusts the prose over the tool
//! spends the refusal looking at the wrong thing. Ask the tool instead:
//! `read-boot-report.py --addr-only <elf>` prints the address AND the length,
//! from the ELF's own symbol size.
//!
//! # Adding a field (phase-460 W5, for W7 and after)
//!
//! The record is meant to grow, and the order it grows in is the whole
//! contract: `read-boot-report.py` decodes POSITIONALLY. So a new field is
//! APPENDED -- after the last field, never inserted between two -- and the
//! four edits are one commit:
//!
//! 1. the field on [`BootReport`], its zero in `BootReport::new`, the same
//!    name in the same position on [`Snapshot`], and its load in [`snapshot`];
//! 2. [`VERSION`] bumped, so an older decoder REFUSES the record instead of
//!    reading the new word as one it knows;
//! 3. `FIELDS` in `scripts/read-boot-report.py`, same name, same position, and
//!    `KNOWN_VERSION` to match;
//! 4. the field count in `the_record_is_twenty_three_packed_u32s` below.
//!
//! `check-boot-report-layout` fails on 1 without 3, and the Rust test fails if
//! the compiler laid the record out with padding. Appending is what keeps a
//! dump taken from an older image decodable by eye: every word before the new
//! one is still where it was, and the version says which words exist.

#![allow(clippy::module_name_repetitions)]

/// `"NRSR"` -- nano-ros self report. Written LAST, so a reader that finds it
/// knows every field before it is already valid.
pub const MAGIC: u32 = 0x4e52_5352;

/// Layout version. Bump on any field change; a reader refuses what it does not
/// know rather than decoding a record it would misread.
///
/// 4 since phase-460 W5 appended `heap_peak_bytes` and `heap_capacity_bytes`;
/// 5 since phase-460 W7 appended `samples_dropped_too_small`.
pub const VERSION: u32 = 5;

/// How far boot got. Monotonic, and the single most useful field: an arena
/// failure halts during entity creation, so the stage that was NOT reached
/// names the phase to look at.
///
/// Numbers follow EXECUTION ORDER, because `checkpoint` keeps the maximum
/// and a stage that runs earlier but numbers higher would make the record
/// claim less progress than was made. Inserting one therefore renumbers the
/// rest and bumps [`VERSION`]; the decoder refuses a version it does not
/// know rather than misreading it, which is what makes that safe.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum Stage {
    /// RAM as the loader left it. Never stored; a reader seeing this with a
    /// valid magic has found a record that was reset but not re-entered.
    Untouched = 0,
    /// The record itself is initialised and the compile-time knobs are in it.
    ///
    /// Stamped at the TOP of the C++ entry point, before any argument is
    /// validated, so "the image never entered nano-ros" is distinguishable
    /// from "it entered and died before the executor". Version 1 stamped this
    /// inside the executor constructor instead, which made those two cases
    /// identical -- both read magic 0 -- and cost a disassembly walk to tell
    /// apart on the first board run.
    ReportReady = 1,
    /// The boot config resolved: node name, namespace, locator and domain id
    /// all parsed. Everything between here and [`Stage::ReportReady`] is
    /// argument validation, and [`BootReport::cpp_init_ret`] says which check
    /// rejected it.
    BootConfigResolved = 2,
    /// An `Executor` has bound its arena, so [`BootReport::arena_capacity`]
    /// is the real slice length rather than the compiled constant.
    ExecutorReady = 3,
    /// Entity registration has begun: something claimed arena bytes.
    ///
    /// Stamped by [`note_alloc`] and [`note_alloc_failed`], which is to say by
    /// the arena allocator itself rather than by any register seam. The arena
    /// is claimed by entity registration and by nothing else, so an allocation
    /// ATTEMPT is the event, and putting the stamp on the record's own writers
    /// means every present and future `arena_alloc*` call site is covered by
    /// construction -- there is no second place to remember.
    ///
    /// Issue 1036: nothing emitted this for three phases, so an image that ran
    /// out of arena left the stage reading [`Stage::ExecutorReady`] -- the same
    /// value as an image that opened its executor and died before registering
    /// anything at all. The record named the allocation and could not say
    /// WHERE, which is half of a diagnostic on a board where it is the only
    /// channel.
    ///
    /// The interval between this and [`Stage::FirstSpin`] is where an
    /// under-sized arena halts, so `stage == 4` with a non-zero
    /// [`BootReport::failed_alloc_shortfall`] is the signature of the failure
    /// this whole record exists to catch.
    RegisteringEntities = 4,
    /// RESERVED. Nothing emits this, and nothing can from inside the core.
    ///
    /// "Every entity the image declares was registered successfully" has no
    /// observable moment here: an application may register lazily, which is the
    /// same reason issue 0900's headroom advisory fires at the first spin
    /// rather than at an end of registration that does not exist. A stamp at
    /// the top of `spin_once` would be a claim the core cannot support.
    ///
    /// Kept rather than removed, and DOCUMENTED rather than left looking
    /// unfinished (issue 1036): removing it renumbers [`Stage::FirstSpin`] and
    /// bumps [`VERSION`] to retire a value no image can produce, and the next
    /// reader would file the renumbering as the bug. An entry shape that DOES
    /// know when its register pass ended -- a generated component `setup`
    /// callback returning OK -- can stamp it without moving anything.
    EntitiesReady = 5,
    /// The first `spin_once` was entered, which is where issue 0900's
    /// headroom advisory would have printed had a sink existed.
    ///
    /// Because [`Stage::EntitiesReady`] has no producer, this is also what
    /// "registration finished" reads as: reaching 6 means the application
    /// stopped registering and started spinning.
    FirstSpin = 6,
}

#[cfg(nros_boot_report)]
pub use enabled::*;

#[cfg(nros_boot_report)]
mod enabled {
    use super::{MAGIC, Stage, VERSION};
    use portable_atomic::{AtomicU32, Ordering};

    /// The record. One per image, in `.bss`.
    ///
    /// `#[repr(C)]` with every field an `AtomicU32` -- which is
    /// `repr(transparent)` over `u32` -- so the layout is exactly the sequence
    /// of 32-bit words the reader script decodes, on every target this crate
    /// builds for.
    ///
    /// Atomics rather than a `static mut` because the record is written from
    /// registration paths that an application may reach from more than one
    /// thread. `Relaxed` throughout: there is no ordering relationship to
    /// establish with any other data, and the reader is a debugger that has
    /// already halted the core.
    #[repr(C)]
    pub struct BootReport {
        magic: AtomicU32,
        version: AtomicU32,
        struct_size: AtomicU32,
        stage: AtomicU32,

        // Compile-time, so that comparing these against what the build system
        // BELIEVES it delivered turns a "derived value did not arrive" defect
        // into a measurement. `scripts/check-knob-delivery.py` asserts the same
        // identity one step earlier, at `build.ninja`; this is the same
        // assertion made by the silicon.
        arena_size: AtomicU32,
        max_cbs: AtomicU32,
        max_sc: AtomicU32,
        max_nodes: AtomicU32,
        default_rx_buf_size: AtomicU32,

        // Runtime.
        arena_capacity: AtomicU32,
        arena_used: AtomicU32,
        alloc_count: AtomicU32,
        last_alloc_size: AtomicU32,
        /// Bytes the allocation that FAILED asked for, or 0 if none has.
        failed_alloc_size: AtomicU32,
        /// Bytes by which that allocation overran the arena. This is the
        /// number to add to `NROS_EXECUTOR_ARENA_SIZE`, which is why it is
        /// stored rather than left to be recomputed from the two above.
        failed_alloc_shortfall: AtomicU32,
        /// `nros_cpp_init`'s return code, as the two's-complement bits of an
        /// `i32`, or 0 (`NROS_CPP_RET_OK`) if it has not returned yet.
        ///
        /// The stage says HOW FAR init got; this says why it stopped. Without
        /// it, every early return in that function -- a null argument, a
        /// non-UTF-8 name, a bad domain id, a backend that refused to open --
        /// is one indistinguishable "did not reach the executor".
        cpp_init_ret: AtomicU32,
        /// The LAST `NodeError` that crossed the C++ FFI, as a stable code.
        ///
        /// Stable here means assigned by `nros-cpp`'s exhaustive mapper, not
        /// taken from the Rust discriminant -- a discriminant shifts whenever a
        /// variant is inserted, and a dump decoded against the wrong numbering
        /// names the wrong error, confidently.
        err_class: AtomicU32,
        /// For [`Self::err_class`] == Transport, which `TransportError`.
        ///
        /// The C++ ABI collapses eight distinct transport variants onto the
        /// single code -100, which is what made an island subscription failure
        /// undiagnosable: the return code said "transport" and nothing said
        /// which. This is the field that separates them.
        err_transport: AtomicU32,
        /// Address of the `Backend(&'static str)` message, or 0.
        ///
        /// The pointer rather than the text: the message is a static string
        /// already in the image, so copying it would cost the record a buffer
        /// to hold something the reader can fetch. Paired with
        /// [`Self::err_backend_len`].
        err_backend_ptr: AtomicU32,
        /// Length of the message at [`Self::err_backend_ptr`], or 0.
        err_backend_len: AtomicU32,

        // The platform heap -- the arena `nros_platform_alloc` hands out of,
        // which on Zephyr is CONFIG_NROS_ZEPHYR_HEAP_SIZE. Appended by
        // phase-460 W5; see "Adding a field" above for why the two words are
        // at the END.
        /// High-water mark of the platform heap, in bytes, or 0 if this image
        /// never sampled it.
        ///
        /// `peak`, not `used`: `used` read at an arbitrary instant reports
        /// whatever happened to be live at the moment of the read, which is
        /// not what a size knob bounds. Issue 1424 -- the reporter behind this
        /// (`nros_zephyr_heap_peak`) has existed and been compiled into every
        /// Zephyr image since phase-412, and nothing read it off a board with
        /// no console, so the one knob it could have sized
        /// (`CONFIG_NROS_ZEPHYR_HEAP_SIZE`) was set by hand and annotated as a
        /// guess.
        ///
        /// MONOTONIC here as well as at the source, so a later sample of a
        /// heap that has since freed memory cannot lower the figure the knob
        /// is sized from.
        heap_peak_bytes: AtomicU32,
        /// Bytes the platform heap was given, or 0 if never sampled.
        ///
        /// The pair is what makes the peak actionable: a peak alone says how
        /// much was needed and says nothing about how close to the edge the
        /// image ran. `capacity - peak` is the headroom the gate refuses on.
        heap_capacity_bytes: AtomicU32,

        /// Samples received, ACKed and then THROWN AWAY because the
        /// subscription's buffer was smaller than the sample. Appended by
        /// phase-460 W7; see "Adding a field" above for why it is at the END.
        ///
        /// Issue 1425. A drop is a QoS fact, not a fault -- nothing halts --
        /// which is exactly why it needs a record: the image keeps running,
        /// every outside probe reports the subscription matched and healthy
        /// (the transport completed and ACKed the sample before the buffer was
        /// consulted), and the application simply never sees the message. On a
        /// board with a console the drop announces itself in a log line. On the
        /// island's target there is no console, so before this field the only
        /// evidence of a dropped 13.4 KiB trajectory was that nothing arrived.
        ///
        /// A TOTAL, not a per-entity figure, for the reason
        /// `executor::arena`'s `DROPPED_TAKES` is one: a per-entity counter is
        /// a field on the arena's entry structs, which are sized by knob at
        /// build time, so it would move every image's executor footprint to buy
        /// a diagnostic. Non-zero here means "read the log lines, or raise
        /// `NROS_SUBSCRIPTION_BUFFER_SIZE` and see whether it goes to zero".
        samples_dropped_too_small: AtomicU32,
    }

    impl BootReport {
        const fn new() -> Self {
            Self {
                magic: AtomicU32::new(0),
                version: AtomicU32::new(0),
                struct_size: AtomicU32::new(0),
                stage: AtomicU32::new(0),
                arena_size: AtomicU32::new(0),
                max_cbs: AtomicU32::new(0),
                max_sc: AtomicU32::new(0),
                max_nodes: AtomicU32::new(0),
                default_rx_buf_size: AtomicU32::new(0),
                arena_capacity: AtomicU32::new(0),
                arena_used: AtomicU32::new(0),
                alloc_count: AtomicU32::new(0),
                last_alloc_size: AtomicU32::new(0),
                failed_alloc_size: AtomicU32::new(0),
                failed_alloc_shortfall: AtomicU32::new(0),
                cpp_init_ret: AtomicU32::new(0),
                err_class: AtomicU32::new(0),
                err_transport: AtomicU32::new(0),
                err_backend_ptr: AtomicU32::new(0),
                err_backend_len: AtomicU32::new(0),
                heap_peak_bytes: AtomicU32::new(0),
                heap_capacity_bytes: AtomicU32::new(0),
                samples_dropped_too_small: AtomicU32::new(0),
            }
        }

        /// Size of the record in bytes, as the reader must expect it.
        ///
        /// ASKED OF THE COMPILER, not counted by hand. A hand-written word
        /// count is a second statement of the field list that drifts the first
        /// time a field is added, and it would drift SILENTLY -- the reader
        /// would accept the record and decode one field short. This whole
        /// campaign is about not hand-picking numbers the build already knows.
        #[must_use]
        pub const fn struct_size() -> u32 {
            // The record is all `AtomicU32`, so this is exact on every target
            // and there is no padding for the cast to lose.
            core::mem::size_of::<Self>() as u32
        }
    }

    /// A plain-value copy of the record.
    ///
    /// Field-for-field with [`BootReport`] and in the SAME ORDER, because
    /// `scripts/read-boot-report.py` decodes that order out of a memory dump.
    /// A test that reads through this therefore exercises the same layout the
    /// script assumes, which is the only thing keeping the two in step.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct Snapshot {
        pub magic: u32,
        pub version: u32,
        pub struct_size: u32,
        pub stage: u32,
        pub arena_size: u32,
        pub max_cbs: u32,
        pub max_sc: u32,
        pub max_nodes: u32,
        pub default_rx_buf_size: u32,
        pub arena_capacity: u32,
        pub arena_used: u32,
        pub alloc_count: u32,
        pub last_alloc_size: u32,
        pub failed_alloc_size: u32,
        pub failed_alloc_shortfall: u32,
        pub cpp_init_ret: u32,
        pub err_class: u32,
        pub err_transport: u32,
        pub err_backend_ptr: u32,
        pub err_backend_len: u32,
        pub heap_peak_bytes: u32,
        pub heap_capacity_bytes: u32,
        pub samples_dropped_too_small: u32,
    }

    /// Read the record.
    #[must_use]
    pub fn snapshot() -> Snapshot {
        let r = &NROS_BOOT_REPORT;
        let g = |f: &AtomicU32| f.load(Ordering::Relaxed);
        Snapshot {
            magic: g(&r.magic),
            version: g(&r.version),
            struct_size: g(&r.struct_size),
            stage: g(&r.stage),
            arena_size: g(&r.arena_size),
            max_cbs: g(&r.max_cbs),
            max_sc: g(&r.max_sc),
            max_nodes: g(&r.max_nodes),
            default_rx_buf_size: g(&r.default_rx_buf_size),
            arena_capacity: g(&r.arena_capacity),
            arena_used: g(&r.arena_used),
            alloc_count: g(&r.alloc_count),
            last_alloc_size: g(&r.last_alloc_size),
            failed_alloc_size: g(&r.failed_alloc_size),
            failed_alloc_shortfall: g(&r.failed_alloc_shortfall),
            cpp_init_ret: g(&r.cpp_init_ret),
            err_class: g(&r.err_class),
            err_transport: g(&r.err_transport),
            err_backend_ptr: g(&r.err_backend_ptr),
            err_backend_len: g(&r.err_backend_len),
            heap_peak_bytes: g(&r.heap_peak_bytes),
            heap_capacity_bytes: g(&r.heap_capacity_bytes),
            samples_dropped_too_small: g(&r.samples_dropped_too_small),
        }
    }

    /// The record, findable by symbol name from a debugger.
    ///
    /// `#[used]` because nothing in a minimal image necessarily reads it, and a
    /// static whose only writes are through this module's functions is exactly
    /// what a linker is entitled to discard.
    #[unsafe(no_mangle)]
    #[used]
    pub static NROS_BOOT_REPORT: BootReport = BootReport::new();

    /// Stamp the header and the compile-time knobs.
    ///
    /// Idempotent, and safe to call from more than one place -- an image with
    /// two executors should not have to decide which one owns the record.
    /// MAGIC is stored LAST so a reader that finds it knows the rest is there.
    pub fn init() {
        let r = &NROS_BOOT_REPORT;
        r.version.store(VERSION, Ordering::Relaxed);
        r.struct_size
            .store(BootReport::struct_size(), Ordering::Relaxed);
        r.arena_size
            .store(saturate(crate::config::ARENA_SIZE), Ordering::Relaxed);
        r.max_cbs
            .store(saturate(crate::config::MAX_CBS), Ordering::Relaxed);
        r.max_sc
            .store(saturate(crate::config::MAX_SC), Ordering::Relaxed);
        r.max_nodes
            .store(saturate(crate::config::MAX_NODES), Ordering::Relaxed);
        r.default_rx_buf_size.store(
            saturate(crate::config::DEFAULT_RX_BUF_SIZE),
            Ordering::Relaxed,
        );
        r.magic.store(MAGIC, Ordering::Relaxed);
        checkpoint(Stage::ReportReady);
    }

    /// Record that boot reached `stage`.
    ///
    /// MONOTONIC: a lower stage never overwrites a higher one, so a late call
    /// on a re-entered path cannot make the record claim less progress than was
    /// actually made. That matters because the field's whole purpose is to be
    /// believed about a boot that did not finish.
    pub fn checkpoint(stage: Stage) {
        let want = stage as u32;
        let r = &NROS_BOOT_REPORT;
        let mut cur = r.stage.load(Ordering::Relaxed);
        while want > cur {
            match r
                .stage
                .compare_exchange_weak(cur, want, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return,
                Err(actual) => cur = actual,
            }
        }
    }

    /// Record the arena slice an `Executor` actually bound.
    ///
    /// Not the same number as `ARENA_SIZE`, and the difference is a finding
    /// rather than noise: the arena's placement is the caller's choice
    /// (issue 0900), so an image can compile one size and hand the executor
    /// another. Both are in the record so a dump can say which happened.
    pub fn note_arena_capacity(capacity: usize) {
        NROS_BOOT_REPORT
            .arena_capacity
            .store(saturate(capacity), Ordering::Relaxed);
    }

    /// Record a successful arena allocation.
    ///
    /// Also stamps [`Stage::RegisteringEntities`]: see that variant for why the
    /// stamp belongs to the record's writers rather than to a register seam.
    pub fn note_alloc(size: usize, used_after: usize) {
        let r = &NROS_BOOT_REPORT;
        r.alloc_count.fetch_add(1, Ordering::Relaxed);
        r.last_alloc_size.store(saturate(size), Ordering::Relaxed);
        r.arena_used.store(saturate(used_after), Ordering::Relaxed);
        checkpoint(Stage::RegisteringEntities);
    }

    /// Record the arena allocation that did not fit.
    ///
    /// FIRST writer wins, on the same reasoning as [`checkpoint`]: the first
    /// failure is the one that explains the boot, and any later one is a
    /// consequence of it.
    /// Also stamps [`Stage::RegisteringEntities`], and UNCONDITIONALLY -- outside
    /// the first-writer branch. A second failure adds nothing to the numbers but
    /// it is still evidence that registration was in flight, and a stage that
    /// depended on winning a race would be exactly the sort of number this
    /// record must never print.
    pub fn note_alloc_failed(size: usize, shortfall: usize) {
        let r = &NROS_BOOT_REPORT;
        checkpoint(Stage::RegisteringEntities);
        if r.failed_alloc_size
            .compare_exchange(0, saturate(size), Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            r.failed_alloc_shortfall
                .store(saturate(shortfall), Ordering::Relaxed);
        }
    }

    /// Record the PLATFORM HEAP allocation that did not fit.
    ///
    /// phase-460 W7 / issue 1425. `nros-platform-zephyr`'s `platform.c` calls
    /// this through the C entry below from `nros_platform_alloc`'s exhaustion
    /// path, BEFORE the printk and before the fatal hook, so a board halted by
    /// an exhausted heap names the request that killed it in the same two words
    /// an exhausted executor ARENA does.
    ///
    /// ONE PAIR OF FIELDS FOR TWO ARENAS. They cannot both fail in a boot that
    /// continues -- the first one to fail stops it -- and first-writer-wins is
    /// what makes the record name the cause rather than a consequence.
    ///
    /// NO STAGE STAMP, which is the whole reason this is not
    /// [`note_alloc_failed`]. That one stamps [`Stage::RegisteringEntities`]
    /// because the executor arena is claimed by entity registration and by
    /// nothing else. The platform heap is claimed by anything at any time --
    /// zenoh-pico's read task, a reply buffer, a reconnect -- so stamping would
    /// make the record claim registration was in flight on a boot that had long
    /// since reached its first spin. `checkpoint` is monotonic, so it would be
    /// an unfalsifiable claim: the stage would read 4 and nothing later could
    /// correct it.
    pub fn note_heap_alloc_failed(size: usize) {
        let r = &NROS_BOOT_REPORT;
        if r.failed_alloc_size
            .compare_exchange(0, saturate(size), Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            // The shortfall is the REQUEST, not `size - free`: the heap is
            // fragmented by the time it refuses, so the bytes that would have
            // made this request fit are not the deficit against the largest
            // free block. A number that looks like a precise deficit and is not
            // would be sized from.
            r.failed_alloc_shortfall
                .store(saturate(size), Ordering::Relaxed);
        }
    }

    /// [`note_heap_alloc_failed`] across the C ABI, for a platform in C.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_boot_report_note_heap_alloc_failed(size: usize) {
        note_heap_alloc_failed(size);
    }

    /// Record `nros_cpp_init`'s return code.
    ///
    /// LAST writer wins, unlike [`note_alloc_failed`]: an image may call
    /// `nros_cpp_init` more than once (per component, per tier), and the
    /// interesting one is the call that did not get through, which is the one
    /// that leaves the stage where it stopped.
    pub fn note_cpp_init_ret(ret: i32) {
        NROS_BOOT_REPORT
            .cpp_init_ret
            .store(ret as u32, Ordering::Relaxed);
    }

    /// Record an error that crossed the FFI.
    ///
    /// Takes CODES, not the error type: `nros-node` must not need to know how
    /// `nros-cpp` numbers its variants, and the numbering has to be assigned by
    /// an exhaustive match that a new variant breaks at compile time. The caller
    /// owns both.
    ///
    /// LAST writer wins. An image that fails one entity and carries on would
    /// otherwise keep the first stumble instead of the one that stopped it, and
    /// the failure that stops setup is the one that explains the boot.
    pub fn note_error(class: u32, transport: u32, backend_ptr: u32, backend_len: u32) {
        let r = &NROS_BOOT_REPORT;
        r.err_class.store(class, Ordering::Relaxed);
        r.err_transport.store(transport, Ordering::Relaxed);
        r.err_backend_ptr.store(backend_ptr, Ordering::Relaxed);
        r.err_backend_len.store(backend_len, Ordering::Relaxed);
    }

    /// Record the platform heap's high-water mark and its capacity.
    ///
    /// `peak` keeps the MAXIMUM, `capacity` the last value: the peak is the
    /// figure a knob is sized from and must not be lowered by a later sample,
    /// while the capacity is a constant of the image that a second sample can
    /// only restate.
    ///
    /// Called from the port that owns the heap, because only that port can
    /// read it: `nros_zephyr_heap_peak()` is deliberately NOT in
    /// `nros/platform.h` (the cross-port ABI would need a stub per port for a
    /// figure one port can produce), so the core cannot pull the number and
    /// the platform pushes it instead. See
    /// [`nros_boot_report_note_heap`] for the C entry point.
    pub fn note_heap(peak: usize, capacity: usize) {
        let r = &NROS_BOOT_REPORT;
        r.heap_peak_bytes
            .fetch_max(saturate(peak), Ordering::Relaxed);
        r.heap_capacity_bytes
            .store(saturate(capacity), Ordering::Relaxed);
    }

    /// [`note_heap`] across the C ABI, for a platform written in C.
    ///
    /// `nros-platform-zephyr`'s `platform.c` calls this from
    /// `nros_platform_alloc` and `nros_platform_realloc` -- the only two
    /// places the heap's peak can change -- so the record is current at every
    /// stage transition without the stage transitions having to sample
    /// anything, and it is written on the exhaustion path BEFORE the printk
    /// that path emits, which is the case where the board is about to stop.
    ///
    /// Takes `usize`, not `u32`: the saturation rule belongs on this side
    /// (see [`saturate`]), and a C caster would truncate instead.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_boot_report_note_heap(peak: usize, capacity: usize) {
        note_heap(peak, capacity);
    }

    /// Count one sample dropped because the subscription buffer was too small.
    ///
    /// Returns the total AFTER this drop, so a caller can throttle its log on
    /// the same number the record carries rather than keeping a second one.
    ///
    /// SATURATES at `u32::MAX` instead of wrapping. A wrapping counter can
    /// read 0 on an image that dropped four billion samples, and 0 is the one
    /// answer here that means "nothing is wrong" -- the same rule
    /// [`saturate`] applies to every size in the record, applied to a tally.
    /// `fetch_update` rather than `fetch_add` is what makes that true under
    /// the concurrent callers `take_serialized` has.
    pub fn note_sample_dropped_too_small() -> u32 {
        let r = &NROS_BOOT_REPORT;
        let prev = r
            .samples_dropped_too_small
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            })
            .unwrap_or(u32::MAX);
        prev.saturating_add(1)
    }

    /// [`note_sample_dropped_too_small`] across the C ABI.
    ///
    /// Present for the same reason [`nros_boot_report_note_heap`] is: the
    /// counting side may be a C translation unit, and a symbol that exists in
    /// only one of the two builds is a link error naming nothing in
    /// particular.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_boot_report_note_sample_dropped_too_small() -> u32 {
        note_sample_dropped_too_small()
    }

    /// Preload the drop tally. TEST ONLY.
    ///
    /// The record's fields are private and `mod tests` is a sibling of this
    /// module, not a child, so it cannot reach one. The saturation rule is
    /// worth a test and `u32::MAX` is not reachable by calling the writer, so
    /// the seam is this, compiled only under `cfg(test)`.
    #[cfg(test)]
    pub fn set_samples_dropped_too_small_for_test(v: u32) {
        NROS_BOOT_REPORT
            .samples_dropped_too_small
            .store(v, Ordering::Relaxed);
    }

    /// `usize` -> `u32`, saturating.
    ///
    /// Every field is a `u32` so the record's layout does not change between a
    /// 32-bit board and a 64-bit host running the same tests. Saturating rather
    /// than truncating because a value too large to represent should read as
    /// "enormous", not as its low half -- a truncated 4 GiB reads as 0, which
    /// is the one wrong answer that looks like a normal one.
    fn saturate(v: usize) -> u32 {
        u32::try_from(v).unwrap_or(u32::MAX)
    }
}

#[cfg(not(nros_boot_report))]
pub use disabled::*;

/// No-op stubs, so call sites need no `cfg` of their own and an image that does
/// not opt in is byte-identical.
#[cfg(not(nros_boot_report))]
mod disabled {
    use super::Stage;

    #[inline(always)]
    pub fn init() {}

    #[inline(always)]
    pub fn checkpoint(_stage: Stage) {}

    #[inline(always)]
    pub fn note_arena_capacity(_capacity: usize) {}

    #[inline(always)]
    pub fn note_alloc(_size: usize, _used_after: usize) {}

    #[inline(always)]
    pub fn note_alloc_failed(_size: usize, _shortfall: usize) {}

    #[inline(always)]
    pub fn note_heap_alloc_failed(_size: usize) {}

    /// The C entry point exists in BOTH builds, for the reason below.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_boot_report_note_heap_alloc_failed(_size: usize) {}

    #[inline(always)]
    pub fn note_cpp_init_ret(_ret: i32) {}

    #[inline(always)]
    pub fn note_error(_class: u32, _transport: u32, _ptr: u32, _len: u32) {}

    #[inline(always)]
    pub fn note_heap(_peak: usize, _capacity: usize) {}

    /// Always 0 without the record, and the CALLER must not care.
    ///
    /// `nros-cpp`'s take path throttles its log on its OWN counter, never on
    /// this return value, precisely so that an image built without the report
    /// keeps the log behaviour an image built with it has. A stub that decided
    /// the throttle would make the diagnostic depend on the diagnostic.
    #[inline(always)]
    pub fn note_sample_dropped_too_small() -> u32 {
        0
    }

    /// The C entry point exists in BOTH builds, unlike every other stub here.
    ///
    /// The Rust half of an image takes the record from a cfg and the C half
    /// takes it from `CONFIG_NROS_BOOT_REPORT`; the two are set from the same
    /// knob and can still be built apart (a stale `cargo` artifact, a C smoke
    /// test). An absent symbol makes that a LINK failure naming nothing in
    /// particular, so the disabled build keeps an empty body instead.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_boot_report_note_heap(_peak: usize, _capacity: usize) {}

    /// The C entry point exists in BOTH builds, for the reason above.
    #[unsafe(no_mangle)]
    pub extern "C" fn nros_boot_report_note_sample_dropped_too_small() -> u32 {
        0
    }
}

#[cfg(all(test, nros_boot_report))]
mod tests {
    use super::*;

    /// The reader decodes twenty-three u32s positionally, so the record must
    /// be exactly that and nothing else -- no padding, no reordering.
    ///
    /// `size_of` on the TARGET, which is the half `check-boot-report-layout.py`
    /// cannot see: that gate compares two source files, and this compares the
    /// source against what the compiler actually laid out.
    #[test]
    fn the_record_is_twenty_three_packed_u32s() {
        assert_eq!(BootReport::struct_size(), 23 * 4);
        assert_eq!(
            core::mem::size_of::<BootReport>(),
            23 * core::mem::size_of::<u32>(),
            "the record grew padding; the reader decodes positionally"
        );
        assert_eq!(core::mem::align_of::<BootReport>(), 4);
    }

    /// The magic is written LAST, so finding it means the rest is valid.
    ///
    /// The reader leans on this to tell "the image died before it had an
    /// executor" apart from "the dump is at the wrong address", and it can
    /// only do so if the ordering actually holds.
    #[test]
    fn init_stamps_the_header_and_the_compiled_knobs() {
        init();
        let s = snapshot();
        assert_eq!(s.magic, MAGIC);
        assert_eq!(s.version, VERSION);
        assert_eq!(s.struct_size, BootReport::struct_size());
        assert_eq!(s.arena_size, crate::config::ARENA_SIZE as u32);
        assert_eq!(s.max_cbs, crate::config::MAX_CBS as u32);
        assert_eq!(s.max_nodes, crate::config::MAX_NODES as u32);
        assert!(s.stage >= Stage::ReportReady as u32);
    }

    /// A late call on a re-entered path must not make the record claim LESS
    /// progress than was actually made. The field's whole purpose is to be
    /// believed about a boot that did not finish.
    #[test]
    fn a_checkpoint_never_goes_backwards() {
        init();
        checkpoint(Stage::FirstSpin);
        assert_eq!(snapshot().stage, Stage::FirstSpin as u32);
        checkpoint(Stage::ExecutorReady);
        assert_eq!(
            snapshot().stage,
            Stage::FirstSpin as u32,
            "an earlier stage overwrote a later one"
        );
    }

    /// The FIRST failure is the one that explains the boot; a later one is a
    /// consequence of it and must not overwrite the cause.
    #[test]
    fn the_first_alloc_failure_wins() {
        init();
        note_alloc_failed(100, 8);
        note_alloc_failed(999, 512);
        let s = snapshot();
        assert_eq!(s.failed_alloc_size, 100);
        assert_eq!(s.failed_alloc_shortfall, 8);
    }

    /// A value too large for the field must read as enormous, not as its low
    /// half. A truncated 4 GiB reads as 0, which is the one wrong answer that
    /// looks like a normal one.
    #[test]
    fn an_unrepresentable_size_saturates_rather_than_truncating() {
        init();
        note_arena_capacity(usize::MAX);
        assert_eq!(snapshot().arena_capacity, u32::MAX);
    }

    /// The heap PEAK is what sizes the knob, so a later sample taken after the
    /// image freed memory must not lower it. Issue 1424: the figure only has
    /// to be believed about a boot nobody watched.
    ///
    /// Through the C entry point, which is how the only producer in the tree
    /// (`nros-platform-zephyr/src/platform.c`) reaches it -- a test that wrote
    /// through `note_heap` would leave that symbol exercised by nothing.
    #[test]
    fn the_heap_peak_keeps_the_maximum_and_the_capacity_the_last_word() {
        init();
        nros_boot_report_note_heap(4096, 94208);
        nros_boot_report_note_heap(18352, 94208);
        nros_boot_report_note_heap(1024, 94208);
        let s = snapshot();
        assert_eq!(s.heap_peak_bytes, 18352, "a later sample lowered the peak");
        assert_eq!(s.heap_capacity_bytes, 94208);
    }

    /// Issue 1425 -- a sample dropped for being too big for its buffer is
    /// COUNTED, and the count is in the one record a console-less board can be
    /// asked for.
    ///
    /// Through the C entry point, which is the one `nros-cpp`'s take path
    /// reaches, and asserting the RETURNED total as well as the record: the
    /// caller throttles its log on that return, so a writer that stored
    /// correctly and returned garbage would log on the wrong samples while
    /// this test read the field and passed.
    #[test]
    fn a_sample_dropped_for_being_too_small_is_counted_in_the_record() {
        init();
        let before = snapshot().samples_dropped_too_small;
        assert_eq!(
            nros_boot_report_note_sample_dropped_too_small(),
            before + 1,
            "the writer must return the total AFTER its own drop"
        );
        assert_eq!(snapshot().samples_dropped_too_small, before + 1);
        assert_eq!(nros_boot_report_note_sample_dropped_too_small(), before + 2);
        assert_eq!(
            snapshot().samples_dropped_too_small,
            before + 2,
            "the second drop did not reach the record"
        );
    }

    /// A tally that WRAPS reads 0 on an image that dropped four billion
    /// samples, and 0 is the one answer here that means nothing is wrong.
    ///
    /// The same rule as [`an_unrepresentable_size_saturates_rather_than_truncating`],
    /// applied to a counter rather than a size. Written through the record
    /// directly because reaching `u32::MAX` by calling the writer is not a test
    /// anyone can run.
    #[test]
    fn the_drop_tally_saturates_rather_than_wrapping_to_zero() {
        init();
        set_samples_dropped_too_small_for_test(u32::MAX);
        assert_eq!(note_sample_dropped_too_small(), u32::MAX);
        assert_eq!(
            snapshot().samples_dropped_too_small,
            u32::MAX,
            "the tally wrapped; a dropping image would read as a healthy one"
        );
        // Leave the record where the other tests in this binary expect it.
        set_samples_dropped_too_small_for_test(0);
    }
}
