//! USB VID:PID and serial-port lookup (RFC-0118 D6).
//!
//! A board descriptor names its console's USB identity (`usb = "0483:374b"`);
//! `nros bench scan` and the port-resolution ladder match attached serial
//! devices against it. The rule this module keeps is RFC-0118 D6's last one:
//! **never guess between two devices.** [`find_port`] answers found, not found
//! or ambiguous, and the caller refuses on the last two.
//!
//! Enumeration reads Linux sysfs (`/sys/class/tty`) and the stable
//! `/dev/serial/by-id` links. On other hosts [`list_serial_ports`] returns an
//! empty list today, which resolves to "not found" and so to a refusal that
//! asks for `--port`, never to a wrong device.

use std::{
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

/// A USB vendor and product id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UsbId {
    /// Vendor id.
    pub vid: u16,
    /// Product id.
    pub pid: u16,
}

/// A `VID:PID` string that does not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbIdError(pub String);

impl fmt::Display for UsbIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} is not a USB id; expected VID:PID in hex, e.g. 0483:374b",
            self.0
        )
    }
}

impl std::error::Error for UsbIdError {}

impl FromStr for UsbId {
    type Err = UsbIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || UsbIdError(s.to_string());
        let (v, p) = s.trim().split_once(':').ok_or_else(err)?;
        let hex = |x: &str| {
            let x = x.strip_prefix("0x").unwrap_or(x);
            if x.is_empty() || x.len() > 4 {
                return None;
            }
            u16::from_str_radix(x, 16).ok()
        };
        Ok(UsbId {
            vid: hex(v).ok_or_else(err)?,
            pid: hex(p).ok_or_else(err)?,
        })
    }
}

impl fmt::Display for UsbId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}:{:04x}", self.vid, self.pid)
    }
}

/// One attached serial device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialPort {
    /// The device node (`/dev/ttyACM0`).
    pub device: PathBuf,
    /// Its `/dev/serial/by-id/…` link, stable across replug, when one exists.
    pub by_id: Option<PathBuf>,
    /// The USB identity, for a USB device.
    pub usb: Option<UsbId>,
    /// The USB serial number, stable across replug and ports.
    pub serial_number: Option<String>,
    /// The USB manufacturer string.
    pub manufacturer: Option<String>,
    /// The USB product string.
    pub product: Option<String>,
}

impl SerialPort {
    /// The path to hand a tool: the by-id link when there is one, since a
    /// `/dev/ttyACM<n>` number can change on replug.
    #[must_use]
    pub fn stable_path(&self) -> &Path {
        self.by_id.as_deref().unwrap_or(&self.device)
    }
}

/// The attached serial devices, sorted by device path.
///
/// # Errors
///
/// An I/O failure reading sysfs.
pub fn list_serial_ports() -> std::io::Result<Vec<SerialPort>> {
    if cfg!(target_os = "linux") {
        list_serial_ports_in(Path::new("/sys/class/tty"), Path::new("/dev"))
    } else {
        Ok(Vec::new())
    }
}

/// [`list_serial_ports`] against an explicit sysfs class directory and
/// `/dev`, so the walk can be tested against a fake tree.
///
/// A tty counts only when it has a `device` link, which is what separates a
/// real port from the virtual consoles (`tty0`…`tty63`) and ptys.
///
/// # Errors
///
/// An I/O failure reading `class_tty`. A missing directory is an empty list.
pub fn list_serial_ports_in(class_tty: &Path, dev: &Path) -> std::io::Result<Vec<SerialPort>> {
    let entries = match std::fs::read_dir(class_tty) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let by_id = by_id_links(&dev.join("serial/by-id"));
    let mut ports = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(device_dir) = entry.path().join("device").canonicalize() else {
            continue;
        };
        let usb_dir = usb_device_dir(&device_dir);
        let read = |f: &str| {
            usb_dir
                .as_ref()
                .and_then(|d| std::fs::read_to_string(d.join(f)).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let usb = match (read("idVendor"), read("idProduct")) {
            (Some(v), Some(p)) => format!("{v}:{p}").parse().ok(),
            _ => None,
        };
        ports.push(SerialPort {
            device: dev.join(&name),
            by_id: by_id
                .iter()
                .find(|(target, _)| *target == name)
                .map(|(_, link)| link.clone()),
            usb,
            serial_number: read("serial"),
            manufacturer: read("manufacturer"),
            product: read("product"),
        });
    }
    ports.sort_by(|a, b| a.device.cmp(&b.device));
    Ok(ports)
}

/// The USB device directory above a tty's `device` directory: the first
/// ancestor holding `idVendor`. An ACM tty's `device` is the USB interface
/// (`1-1:1.0`), whose parent is the device; a USB-serial converter's sits one
/// level lower (`1-1:1.0/ttyUSB0`).
fn usb_device_dir(device_dir: &Path) -> Option<PathBuf> {
    device_dir
        .ancestors()
        .take(4)
        .find(|d| d.join("idVendor").is_file())
        .map(Path::to_path_buf)
}

/// `(tty name, by-id link)` for every link in `/dev/serial/by-id`.
fn by_id_links(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let target = std::fs::read_link(e.path()).ok()?;
            let name = target.file_name()?.to_string_lossy().into_owned();
            Some((name, e.path()))
        })
        .collect();
    out.sort();
    out
}

/// The answer to "which port is this board on".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortLookup {
    /// Exactly one device matched.
    Found(SerialPort),
    /// None matched.
    NotFound,
    /// Several matched; the caller must refuse and list them.
    Ambiguous(Vec<SerialPort>),
}

/// Find the one port matching `usb`, narrowed by `serial_number` when given
/// (the bench file's stable identity). Never picks between two matches.
#[must_use]
pub fn find_port(ports: &[SerialPort], usb: UsbId, serial_number: Option<&str>) -> PortLookup {
    let mut hits: Vec<SerialPort> = ports
        .iter()
        .filter(|p| p.usb == Some(usb))
        .filter(|p| serial_number.is_none_or(|s| p.serial_number.as_deref() == Some(s)))
        .cloned()
        .collect();
    match hits.len() {
        0 => PortLookup::NotFound,
        1 => PortLookup::Found(hits.remove(0)),
        _ => PortLookup::Ambiguous(hits),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_ids_parse_and_print() {
        let id: UsbId = "0483:374B".parse().unwrap();
        assert_eq!(
            id,
            UsbId {
                vid: 0x0483,
                pid: 0x374b
            }
        );
        assert_eq!(id.to_string(), "0483:374b");
        assert_eq!("0x303a:0x1001".parse::<UsbId>().unwrap().vid, 0x303a);
        for bad in ["0483", "0483:", ":374b", "12345:1", "zz:00", ""] {
            assert!(bad.parse::<UsbId>().is_err(), "{bad}");
        }
    }

    fn port(dev: &str, usb: &str, serial: &str) -> SerialPort {
        SerialPort {
            device: dev.into(),
            by_id: None,
            usb: Some(usb.parse().unwrap()),
            serial_number: Some(serial.into()),
            manufacturer: None,
            product: None,
        }
    }

    #[test]
    fn lookup_never_guesses() {
        let st = "0483:374b".parse().unwrap();
        let ports = [
            port("/dev/ttyACM0", "0483:374b", "A"),
            port("/dev/ttyACM1", "0483:374b", "B"),
            port("/dev/ttyUSB0", "0403:6001", "C"),
        ];
        assert!(matches!(find_port(&ports, st, None), PortLookup::Ambiguous(v) if v.len() == 2));
        assert!(
            matches!(find_port(&ports, st, Some("B")), PortLookup::Found(p) if p.device == Path::new("/dev/ttyACM1"))
        );
        assert_eq!(find_port(&ports, st, Some("Z")), PortLookup::NotFound);
        assert_eq!(
            find_port(&ports, "1234:5678".parse().unwrap(), None),
            PortLookup::NotFound
        );
    }

    #[cfg(unix)]
    #[test]
    fn walks_a_fake_sysfs_tree() {
        use std::{fs, os::unix::fs::symlink};

        let root = std::env::temp_dir().join(format!(
            "nros-extension-usb-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        let devices = root.join("sys/devices/usb1");
        // An ACM device: `device` -> the interface, whose parent has the ids.
        let acm = devices.join("1-1");
        fs::create_dir_all(acm.join("1-1:1.0")).unwrap();
        fs::write(acm.join("idVendor"), "0483\n").unwrap();
        fs::write(acm.join("idProduct"), "374b\n").unwrap();
        fs::write(acm.join("serial"), "066DFF\n").unwrap();
        // A USB-serial converter: `device` sits one level below the interface.
        let ftdi = devices.join("1-2");
        fs::create_dir_all(ftdi.join("1-2:1.0/ttyUSB0")).unwrap();
        fs::write(ftdi.join("idVendor"), "0403\n").unwrap();
        fs::write(ftdi.join("idProduct"), "6001\n").unwrap();

        let class = root.join("sys/class/tty");
        for (name, target) in [
            ("ttyACM0", acm.join("1-1:1.0")),
            ("ttyUSB0", ftdi.join("1-2:1.0/ttyUSB0")),
        ] {
            fs::create_dir_all(class.join(name)).unwrap();
            symlink(&target, class.join(name).join("device")).unwrap();
        }
        // A virtual console: no `device` link, so not a port.
        fs::create_dir_all(class.join("tty0")).unwrap();

        let dev = root.join("dev");
        fs::create_dir_all(dev.join("serial/by-id")).unwrap();
        symlink("../../ttyACM0", dev.join("serial/by-id/usb-ST_STLINK-if02")).unwrap();

        let ports = list_serial_ports_in(&class, &dev).unwrap();
        assert_eq!(ports.len(), 2, "{ports:?}");
        assert_eq!(ports[0].device, dev.join("ttyACM0"));
        assert_eq!(ports[0].usb, Some("0483:374b".parse().unwrap()));
        assert_eq!(ports[0].serial_number.as_deref(), Some("066DFF"));
        assert_eq!(
            ports[0].stable_path(),
            dev.join("serial/by-id/usb-ST_STLINK-if02")
        );
        assert_eq!(ports[1].usb, Some("0403:6001".parse().unwrap()));
        assert_eq!(ports[1].stable_path(), dev.join("ttyUSB0"));

        assert!(
            list_serial_ports_in(&root.join("absent"), &dev)
                .unwrap()
                .is_empty()
        );
        fs::remove_dir_all(&root).unwrap();
    }
}
