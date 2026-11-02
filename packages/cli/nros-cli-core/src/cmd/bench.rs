//! phase-529 W4 — `nros bench scan|list` and port resolution (RFC-0118 D6).
//!
//! The bench file, `$NROS_HOME/bench.toml`, records which attached USB
//! device is which board on THIS machine. One `[[board]]` per device, keyed by
//! the USB serial number (stable across replug and across USB ports), with the
//! `/dev/serial/by-id/` path (stable when the kernel renumbers the tty):
//!
//! ```toml
//! [[board]]
//! name   = "nucleo-f767zi-zephyr"
//! serial = "066DFF494851877267084535"
//! port   = "/dev/serial/by-id/usb-STMicroelectronics_…"
//! probe  = "066DFF494851877267084535"   # optional, hand-added
//! power_cycle = "./relay.sh 3"          # optional, hand-added
//! ```
//!
//! The file belongs to one machine and is never committed. A rescan keeps
//! every hand-added field of an entry it already has (matched by serial).
//!
//! It supplies ports and probes and NOTHING ELSE: whether a test cell runs is
//! a property of the lane (phase-529 W6, issue 1758), never of what this file
//! or the USB bus happens to hold.
//!
//! # Port resolution (D6), first answer wins
//!
//! 1. `--port`;
//! 2. `NROS_PORT` (both the invocation rung of the RunPlan ladder);
//! 3. the bench file's entry for the board, found on the bus by its serial
//!    number;
//! 4. exactly one attached device matching the board's `[board.console] usb`.
//!
//! Two candidates are refused, listing both: nros never guesses between two
//! devices.

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr};
use nros_extension::usb::{PortLookup, SerialPort, UsbId, find_port};
use serde::{Deserialize, Serialize};

/// One recorded device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchEntry {
    /// The board key it is (a descriptor name).
    pub name: String,
    /// USB serial number — the key.
    pub serial: String,
    /// The stable `/dev/serial/by-id/` path when the device has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    /// The debug probe's identity, hand-added (`probe-rs --probe`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probe: Option<String>,
    /// A host command that power-cycles the board, hand-added.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power_cycle: Option<String>,
    /// Anything else a person wrote, kept through a rescan.
    #[serde(flatten)]
    pub other: toml::Table,
}

/// `$NROS_HOME/bench.toml`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BenchFile {
    #[serde(default, rename = "board", skip_serializing_if = "Vec::is_empty")]
    pub boards: Vec<BenchEntry>,
}

/// Where the bench file lives: `$NROS_HOME/bench.toml`.
#[must_use]
pub fn path() -> PathBuf {
    crate::orchestration::store::root().join("bench.toml")
}

impl BenchFile {
    /// Read `path`; a missing file is an empty bench.
    ///
    /// # Errors
    ///
    /// The file exists and does not parse.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).wrap_err_with(|| format!("parse {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).wrap_err_with(|| format!("read {}", path.display())),
        }
    }

    /// Write `path`, with a header saying what the file is.
    ///
    /// # Errors
    ///
    /// An I/O failure.
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let body = toml::to_string(self)?;
        std::fs::write(
            path,
            format!(
                "# nros bench file (RFC-0118 D6): which attached device is which board on \
                 THIS machine.\n# Written by `nros bench scan`; hand-added fields \
                 (probe, power_cycle, ...) are kept on rescan. Never commit it.\n\n{body}"
            ),
        )
        .wrap_err_with(|| format!("write {}", path.display()))
    }

    /// The entry recorded for `board`.
    #[must_use]
    pub fn entry(&self, board: &str) -> Option<&BenchEntry> {
        self.boards.iter().find(|e| e.name == board)
    }

    /// Record every attached device whose VID:PID a wanted board states.
    /// Returns one line per device for the scan's report. A device that
    /// matches two boards' VID:PID is not recorded (it cannot say which board
    /// it is); a device with no serial number cannot be keyed and is reported.
    pub fn scan(&mut self, ports: &[SerialPort], wanted: &[(String, UsbId)]) -> Vec<String> {
        let mut report = Vec::new();
        for p in ports {
            let Some(usb) = p.usb else { continue };
            let boards: Vec<&String> = wanted
                .iter()
                .filter(|(_, u)| *u == usb)
                .map(|(n, _)| n)
                .collect();
            let shown = p.stable_path().display().to_string();
            match (boards.as_slice(), &p.serial_number) {
                ([], _) => {}
                ([_, _, ..], _) => report.push(format!(
                    "{shown} ({usb}) matches several boards ({}); add it by hand",
                    boards
                        .iter()
                        .map(|b| b.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
                ([_], None) => report.push(format!(
                    "{shown} ({usb}) has no USB serial number, so it cannot be recorded"
                )),
                ([board], Some(serial)) => {
                    let port = p.by_id.as_ref().map(|b| b.display().to_string());
                    match self.boards.iter_mut().find(|e| &e.serial == serial) {
                        Some(e) => {
                            // A person may have renamed the entry to a more
                            // specific board; keep their name.
                            e.port = port.or_else(|| e.port.take());
                            report.push(format!("{shown}: {} (serial {serial}, kept)", e.name));
                        }
                        None => {
                            self.boards.push(BenchEntry {
                                name: (*board).clone(),
                                serial: serial.clone(),
                                port,
                                probe: None,
                                power_cycle: None,
                                other: toml::Table::new(),
                            });
                            report.push(format!("{shown}: {board} (serial {serial}, new)"));
                        }
                    }
                }
            }
        }
        report
    }
}

/// This machine's bench: the file and what is attached now.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Machine {
    pub file: BenchFile,
    pub ports: Vec<SerialPort>,
}

impl Machine {
    /// The real bench file and the attached serial devices.
    ///
    /// # Errors
    ///
    /// The bench file does not parse, or sysfs cannot be read.
    pub fn load() -> Result<Self> {
        Ok(Self {
            file: BenchFile::load(&path())?,
            ports: nros_extension::usb::list_serial_ports()?,
        })
    }
}

/// Where a port answer came from: the RunPlan rung word.
pub type Rung = &'static str;

/// RFC-0118 D6's port ladder for `board` (whose console states `usb`, when
/// it does). `Ok(None)`: nothing on the ladder names a port (no flag, no
/// entry, no matching device). `Err`: an answer exists and is unsafe to use —
/// two candidates, or a recorded device that is not attached.
///
/// # Errors
///
/// The refusal text, listing what was found.
pub fn resolve_port(
    flag: Option<&str>,
    env: Option<&str>,
    machine: &Machine,
    board: &str,
    usb: Option<UsbId>,
) -> Result<Option<(String, Rung)>, String> {
    use crate::cmd::run_plan::{BENCH, INVOCATION};
    if let Some(p) = flag.or(env) {
        return Ok(Some((p.to_string(), INVOCATION)));
    }
    let listing = |ports: &[SerialPort]| -> String {
        if ports.is_empty() {
            return "no USB serial device is attached".into();
        }
        ports
            .iter()
            .map(|p| {
                format!(
                    "  {} ({}, serial {})",
                    p.stable_path().display(),
                    p.usb.map_or_else(|| "no USB id".into(), |u| u.to_string()),
                    p.serial_number.as_deref().unwrap_or("none")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    if let Some(e) = machine.file.entry(board) {
        let on_bus = machine
            .ports
            .iter()
            .find(|p| p.serial_number.as_deref() == Some(e.serial.as_str()));
        return match on_bus {
            Some(p) => Ok(Some((p.stable_path().display().to_string(), BENCH))),
            None => Err(format!(
                "board `{board}` is recorded in {} as serial {}, and no attached device has \
                 that serial number. Plug it in, or pass `--port`. Attached:\n{}",
                path().display(),
                e.serial,
                listing(&machine.ports)
            )),
        };
    }
    let Some(usb) = usb else {
        return Ok(None);
    };
    match find_port(&machine.ports, usb, None) {
        PortLookup::Found(p) => Ok(Some((p.stable_path().display().to_string(), BENCH))),
        PortLookup::NotFound => Ok(None),
        PortLookup::Ambiguous(hits) => Err(format!(
            "board `{board}`: {} attached devices match its console's USB id {usb}, and \
             nros never guesses between two. Record which is which with `nros bench scan` \
             (then rename an entry if needed), or pass `--port`. Matching:\n{}",
            hits.len(),
            listing(&hits)
        )),
    }
}

// ── `nros bench` ───────────────────────────────────────────────────────

/// `nros bench` — this machine's boards (RFC-0118 D6).
#[derive(clap::Args, Debug)]
pub struct Args {
    #[command(subcommand)]
    pub command: Sub,
}

#[derive(clap::Subcommand, Debug)]
pub enum Sub {
    /// Record the attached devices whose USB id a board's `[board.console]
    /// usb` states, in `$NROS_HOME/bench.toml`.
    Scan(ScanArgs),
    /// Print the bench file and whether each device is attached now.
    List,
}

#[derive(clap::Args, Debug)]
pub struct ScanArgs {
    /// nano-ros checkout (auto-detected if omitted): whose board descriptors
    /// to match against.
    #[arg(long)]
    pub nano_ros_path: Option<PathBuf>,
    /// Print what would be recorded and write nothing.
    #[arg(long)]
    pub dry_run: bool,
}

/// Every board stating a console USB id, as `(name, usb)`.
fn wanted_boards(nano_ros_path: Option<PathBuf>) -> Result<Vec<(String, UsbId)>> {
    let cwd = std::env::current_dir().unwrap_or_default();
    let root = crate::orchestration::nano_ros_root::resolve(nano_ros_path, &cwd)
        .ok_or_else(|| eyre::eyre!("no nano-ros checkout found; pass --nano-ros-path"))?;
    let catalog = crate::orchestration::board_descriptor::BoardCatalog::load(&root)?;
    Ok(catalog
        .descriptors()
        .iter()
        .filter_map(|d| {
            let usb = d.console.as_ref()?.usb?;
            Some((
                d.canonical_name().to_string(),
                UsbId {
                    vid: usb.vid,
                    pid: usb.pid,
                },
            ))
        })
        .collect())
}

/// `nros bench`.
///
/// # Errors
///
/// The bench file or sysfs cannot be read, or the file cannot be written.
pub fn run(args: Args) -> Result<()> {
    let file = path();
    match args.command {
        Sub::Scan(a) => {
            let wanted = wanted_boards(a.nano_ros_path)?;
            let mut bench = BenchFile::load(&file)?;
            let ports = nros_extension::usb::list_serial_ports()?;
            let report = bench.scan(&ports, &wanted);
            for l in &report {
                println!("{l}");
            }
            if report.is_empty() {
                println!(
                    "no attached device matches the USB id of a board's `[board.console] usb` \
                     ({} board(s) state one)",
                    wanted.len()
                );
            }
            if a.dry_run {
                println!("(dry run: {} not written)", file.display());
            } else {
                bench.save(&file)?;
                println!("wrote {}", file.display());
            }
            Ok(())
        }
        Sub::List => {
            let bench = BenchFile::load(&file)?;
            let ports = nros_extension::usb::list_serial_ports()?;
            print!("{}", render_list(&bench, &ports, &file));
            Ok(())
        }
    }
}

/// `nros bench list`'s text.
#[must_use]
pub fn render_list(bench: &BenchFile, ports: &[SerialPort], file: &Path) -> String {
    if bench.boards.is_empty() {
        return format!(
            "{}: no boards recorded (run `nros bench scan`)\n",
            file.display()
        );
    }
    let mut out = format!("{}:\n", file.display());
    for e in &bench.boards {
        let here = ports
            .iter()
            .find(|p| p.serial_number.as_deref() == Some(e.serial.as_str()));
        out.push_str(&format!(
            "  {:<28} serial {:<26} {}\n",
            e.name,
            e.serial,
            match here {
                Some(p) => format!("present at {}", p.stable_path().display()),
                None => "absent".to_string(),
            }
        ));
        for (k, v) in [("probe", &e.probe), ("power_cycle", &e.power_cycle)] {
            if let Some(v) = v {
                out.push_str(&format!("      {k} = {v}\n"));
            }
        }
    }
    out
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use nros_extension::usb::list_serial_ports_in;

    const ST: &str = "0483:374b";

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nros-bench-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A fake sysfs + devtmpfs with one ACM device: `<root>/sys/devices/usb/
    /// <dev>/{idVendor,idProduct,serial}` with its interface `<dev>:1.0`, the
    /// tty `class/tty/<tty>/device` linking to the interface, and the by-id
    /// link (stable across renumbering) pointing at `<root>/dev/<tty>`.
    fn plug(root: &Path, usb_dev: &str, tty: &str, serial: &str) {
        let dev_dir = root.join("sys/devices/usb").join(usb_dev);
        let iface = dev_dir.join(format!("{usb_dev}:1.0"));
        std::fs::create_dir_all(&iface).unwrap();
        std::fs::write(dev_dir.join("idVendor"), "0483\n").unwrap();
        std::fs::write(dev_dir.join("idProduct"), "374b\n").unwrap();
        std::fs::write(dev_dir.join("serial"), format!("{serial}\n")).unwrap();
        let class = root.join("sys/class/tty").join(tty);
        std::fs::create_dir_all(&class).unwrap();
        std::os::unix::fs::symlink(&iface, class.join("device")).unwrap();
        let dev = root.join("dev");
        std::fs::create_dir_all(dev.join("serial/by-id")).unwrap();
        std::fs::write(dev.join(tty), "").unwrap();
        std::os::unix::fs::symlink(
            dev.join(tty),
            dev.join(format!("serial/by-id/usb-STMicro_STLink_{serial}-if02")),
        )
        .unwrap();
    }

    fn unplug(root: &Path, usb_dev: &str, tty: &str, serial: &str) {
        let _ = std::fs::remove_dir_all(root.join("sys/devices/usb").join(usb_dev));
        let _ = std::fs::remove_dir_all(root.join("sys/class/tty").join(tty));
        let _ = std::fs::remove_file(root.join("dev").join(tty));
        let _ = std::fs::remove_file(
            root.join(format!("dev/serial/by-id/usb-STMicro_STLink_{serial}-if02")),
        );
    }

    fn attached(root: &Path) -> Vec<SerialPort> {
        list_serial_ports_in(&root.join("sys/class/tty"), &root.join("dev")).unwrap()
    }

    /// phase-529 W4 acceptance, with no hardware: two matching devices and no
    /// bench entry refuse, listing both; after a scan (and naming which entry
    /// is which board) the recorded one is picked; after a replug onto another
    /// tty number it is still that board, on the same by-id path.
    #[test]
    fn two_devices_refuse_then_scan_picks_and_a_replug_keeps_the_board() {
        let root = tmp("accept");
        plug(&root, "1-1", "ttyACM0", "AAA111");
        plug(&root, "1-2", "ttyACM1", "BBB222");
        let usb: UsbId = ST.parse().unwrap();
        let board = "nucleo-f767zi-zephyr";

        let m = Machine {
            file: BenchFile::default(),
            ports: attached(&root),
        };
        let e = resolve_port(None, None, &m, board, Some(usb)).unwrap_err();
        assert!(
            e.contains("never guesses") && e.contains("AAA111") && e.contains("BBB222"),
            "{e}"
        );

        // Scan records both under the board's name; the person keeps the one
        // on their desk by deleting the other entry (or renaming it).
        let mut file = BenchFile::default();
        let report = file.scan(&m.ports, &[(board.to_string(), usb)]);
        assert_eq!(report.len(), 2, "{report:?}");
        file.boards.retain(|e| e.serial == "BBB222");
        file.boards[0].power_cycle = Some("./relay.sh 3".into());
        let saved = root.join("bench.toml");
        file.save(&saved).unwrap();
        let file = BenchFile::load(&saved).unwrap();

        let m = Machine {
            file: file.clone(),
            ports: attached(&root),
        };
        let (port, rung) = resolve_port(None, None, &m, board, Some(usb))
            .unwrap()
            .unwrap();
        assert_eq!(rung, crate::cmd::run_plan::BENCH);
        assert!(port.ends_with("usb-STMicro_STLink_BBB222-if02"), "{port}");

        // Replug BBB222 onto ttyACM5: same serial, same by-id name, new tty.
        unplug(&root, "1-2", "ttyACM1", "BBB222");
        plug(&root, "1-3", "ttyACM5", "BBB222");
        let ports = attached(&root);
        assert!(ports.iter().any(|p| p.device.ends_with("ttyACM5")));
        let m = Machine { file, ports };
        let (again, _) = resolve_port(None, None, &m, board, Some(usb))
            .unwrap()
            .unwrap();
        assert_eq!(again, port, "the by-id path, not the tty number");

        // `--port` and `NROS_PORT` outrank the bench, in that order.
        let (p, r) = resolve_port(Some("/dev/ttyX"), Some("/dev/ttyY"), &m, board, None)
            .unwrap()
            .unwrap();
        assert_eq!(
            (p.as_str(), r),
            ("/dev/ttyX", crate::cmd::run_plan::INVOCATION)
        );
        let (p, _) = resolve_port(None, Some("/dev/ttyY"), &m, board, None)
            .unwrap()
            .unwrap();
        assert_eq!(p, "/dev/ttyY");

        // Unplugged: the recorded board refuses rather than falling back.
        unplug(&root, "1-3", "ttyACM5", "BBB222");
        let m = Machine {
            file: m.file,
            ports: attached(&root),
        };
        let e = resolve_port(None, None, &m, board, Some(usb)).unwrap_err();
        assert!(
            e.contains("BBB222") && e.contains("no attached device"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// One matching device and no entry is an answer; none is no answer.
    #[test]
    fn a_single_match_answers_and_none_is_no_answer() {
        let root = tmp("single");
        let usb: UsbId = ST.parse().unwrap();
        let empty = Machine::default();
        assert_eq!(resolve_port(None, None, &empty, "b", Some(usb)), Ok(None));
        plug(&root, "1-1", "ttyACM0", "AAA111");
        let m = Machine {
            file: BenchFile::default(),
            ports: attached(&root),
        };
        let (p, _) = resolve_port(None, None, &m, "b", Some(usb))
            .unwrap()
            .unwrap();
        assert!(p.ends_with("AAA111-if02"), "{p}");
        // A board with no console USB id and no entry has nothing to go on.
        assert_eq!(resolve_port(None, None, &m, "b", None), Ok(None));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A rescan keeps hand-added fields and a hand-chosen name.
    #[test]
    fn a_rescan_keeps_what_a_person_wrote() {
        let root = tmp("rescan");
        plug(&root, "1-1", "ttyACM0", "AAA111");
        let usb: UsbId = ST.parse().unwrap();
        let path = root.join("bench.toml");
        std::fs::write(
            &path,
            "[[board]]\nname = \"my-nucleo\"\nserial = \"AAA111\"\nport = \"/old\"\n\
             probe = \"AAA111\"\npower_cycle = \"./relay.sh 3\"\nshelf = \"B2\"\n",
        )
        .unwrap();
        let mut file = BenchFile::load(&path).unwrap();
        file.scan(&attached(&root), &[("nucleo-f767zi-zephyr".into(), usb)]);
        file.save(&path).unwrap();
        let again = BenchFile::load(&path).unwrap();
        let e = &again.boards[0];
        assert_eq!(again.boards.len(), 1);
        assert_eq!(e.name, "my-nucleo");
        assert_eq!(e.power_cycle.as_deref(), Some("./relay.sh 3"));
        assert_eq!(e.probe.as_deref(), Some("AAA111"));
        assert_eq!(e.other.get("shelf").and_then(|v| v.as_str()), Some("B2"));
        assert!(e.port.as_deref().unwrap().ends_with("AAA111-if02"));
        let listed = render_list(&again, &attached(&root), &path);
        assert!(
            listed.contains("my-nucleo") && listed.contains("present at"),
            "{listed}"
        );
        let listed = render_list(&again, &[], &path);
        assert!(listed.contains("absent"), "{listed}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The bench rung in the RunPlan: a flash plan's console port comes from
    /// the machine (`bench`), two candidates refuse the plan, and a `run` plan
    /// never takes a port from the bus.
    #[test]
    fn the_bench_rung_fills_a_flash_plan_and_never_a_run_plan() {
        use crate::cmd::run_plan::{self, ImageTables, Inputs, Invocation};
        use nros_extension::plan::Verb;
        let root = tmp("rung");
        plug(&root, "1-1", "ttyACM0", "AAA111");
        plug(&root, "1-2", "ttyACM1", "BBB222");
        let cat = crate::orchestration::board_descriptor::BoardCatalog::load(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(3)
                .unwrap(),
        )
        .unwrap();
        let mut d = cat
            .descriptors()
            .iter()
            .find(|d| d.names.iter().any(|n| n == "nucleo-f767zi-zephyr"))
            .unwrap()
            .clone();
        d.console = Some(nros_orchestration_ir::target_exec::ConsoleTable {
            kind: Some(nros_orchestration_ir::target_exec::ConsoleKind::Serial),
            usb: Some(ST.parse().unwrap()),
            ..Default::default()
        });
        let tables = ImageTables::default();
        let inv = Invocation::default();
        let board = "nucleo-f767zi-zephyr";
        let mut machine = Machine {
            file: BenchFile::default(),
            ports: attached(&root),
        };
        let e = run_plan::bench_for(Verb::Flash, board, &d, &tables, &inv, &machine).unwrap_err();
        assert!(e.to_string().contains("never guesses"), "{e}");
        assert_eq!(
            run_plan::bench_for(Verb::Run, board, &d, &tables, &inv, &machine).unwrap(),
            run_plan::Bench::default()
        );

        machine.file.scan(
            &machine.ports.clone(),
            &[(board.into(), ST.parse().unwrap())],
        );
        machine.file.boards.retain(|e| e.serial == "AAA111");
        machine.file.boards[0].probe = Some("AAA111".into());
        let bench = run_plan::bench_for(Verb::Flash, board, &d, &tables, &inv, &machine).unwrap();
        let out = crate::cmd::build_next::BuildOutput::West {
            build_dir: Some("/b".into()),
        };
        let plan = run_plan::resolve(
            Verb::Flash,
            Inputs {
                image: "b:nucleo",
                board,
                descriptor: &d,
                output: Some(&out),
                tables: &tables,
                bench: &bench,
                invocation: &inv,
            },
        )
        .unwrap();
        assert!(
            plan.console
                .port
                .as_deref()
                .unwrap()
                .ends_with("AAA111-if02"),
            "{:?}",
            plan.console
        );
        assert_eq!(plan.provenance["console.port"], run_plan::BENCH);
        assert_eq!(plan.args["probe"], "AAA111");
        assert_eq!(plan.provenance["args.probe"], run_plan::BENCH);
        let _ = std::fs::remove_dir_all(&root);
    }
}
