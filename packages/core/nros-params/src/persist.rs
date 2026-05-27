//! Phase 172.H — runtime parameter-override persistence.
//!
//! A [`ParamStore`] persists parameter values set at runtime (via the
//! `set_parameters` service) so they survive a restart. At boot the generated
//! runtime declares the plan's compile-time defaults, then [`ParamStore::load`]
//! overlays any persisted overrides; after a successful runtime set the
//! executor flushes the full parameter set back via [`ParamStore::save`].
//!
//! [`NullParamStore`] is the no-op default (no persistence). [`FileParamStore`]
//! (`std` only) persists scalars to a text file — the hosted backend. Flash /
//! NVS backends for embedded targets are future work.

use crate::types::ParameterValue;

/// Error from a [`ParamStore`] backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamStoreError {
    /// The backend (filesystem, flash, …) reported a failure.
    Backend,
    /// Persisted data was malformed.
    Format,
}

/// Backend that persists runtime parameter overrides across restarts (172.H).
///
/// Object-safe so the executor can hold a `Box<dyn ParamStore>`.
pub trait ParamStore {
    /// Apply each persisted `(name, value)` via `apply`. Called once at boot
    /// after defaults are declared, so persisted values override them.
    fn load(&self, apply: &mut dyn FnMut(&str, ParameterValue));

    /// Persist the full current parameter set. `params` yields `(name, value)`
    /// for every declared parameter; called after a runtime set changes a
    /// value. Non-scalar values (arrays, `NotSet`) are backend-defined and may
    /// be skipped.
    fn save(
        &mut self,
        params: &mut dyn Iterator<Item = (&str, &ParameterValue)>,
    ) -> Result<(), ParamStoreError>;
}

/// No-op store: the default when persistence is disabled.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullParamStore;

impl ParamStore for NullParamStore {
    fn load(&self, _apply: &mut dyn FnMut(&str, ParameterValue)) {}

    fn save(
        &mut self,
        _params: &mut dyn Iterator<Item = (&str, &ParameterValue)>,
    ) -> Result<(), ParamStoreError> {
        Ok(())
    }
}

#[cfg(feature = "std")]
mod file {
    extern crate std;

    use std::{format, fs, io::Write, path::PathBuf, string::String as StdString};

    use super::{ParamStore, ParamStoreError};
    use crate::types::ParameterValue;

    /// Hosted [`ParamStore`] persisting scalar parameter overrides to a text
    /// file: one `name<TAB>kind<TAB>value` line per scalar parameter (kind ∈
    /// `b`/`i`/`d`/`s`). Arrays and `NotSet` are not persisted; names or values
    /// containing a tab or newline are skipped (they would corrupt the line
    /// format). Writes are atomic (temp file + rename).
    #[derive(Debug, Clone)]
    pub struct FileParamStore {
        path: PathBuf,
    }

    impl FileParamStore {
        /// Persist to / restore from `path`.
        pub fn new(path: impl Into<PathBuf>) -> Self {
            Self { path: path.into() }
        }
    }

    impl ParamStore for FileParamStore {
        fn load(&self, apply: &mut dyn FnMut(&str, ParameterValue)) {
            let Ok(raw) = fs::read_to_string(&self.path) else {
                return; // absent / unreadable ⇒ no overrides
            };
            for line in raw.lines() {
                let line = line.trim_end_matches(['\r', '\n']);
                if line.is_empty() {
                    continue;
                }
                let mut parts = line.splitn(3, '\t');
                let (Some(name), Some(kind), Some(val)) =
                    (parts.next(), parts.next(), parts.next())
                else {
                    continue;
                };
                let value = match kind {
                    "b" => match val {
                        "true" => ParameterValue::Bool(true),
                        "false" => ParameterValue::Bool(false),
                        _ => continue,
                    },
                    "i" => match val.parse::<i64>() {
                        Ok(v) => ParameterValue::Integer(v),
                        Err(_) => continue,
                    },
                    "d" => match val.parse::<f64>() {
                        Ok(v) => ParameterValue::Double(v),
                        Err(_) => continue,
                    },
                    "s" => match ParameterValue::from_string(val) {
                        Some(v) => v,
                        None => continue,
                    },
                    _ => continue,
                };
                apply(name, value);
            }
        }

        fn save(
            &mut self,
            params: &mut dyn Iterator<Item = (&str, &ParameterValue)>,
        ) -> Result<(), ParamStoreError> {
            let mut out = StdString::new();
            for (name, value) in params {
                let (kind, rendered): (&str, StdString) = match value {
                    ParameterValue::Bool(b) => {
                        ("b", if *b { "true".into() } else { "false".into() })
                    }
                    ParameterValue::Integer(i) => ("i", format!("{i}")),
                    ParameterValue::Double(d) => ("d", format!("{d}")),
                    ParameterValue::String(s) => ("s", s.as_str().into()),
                    _ => continue, // arrays / NotSet not persisted in v1
                };
                if name.contains(['\t', '\n']) || rendered.contains(['\t', '\n']) {
                    continue; // delimiters in data would corrupt the line format
                }
                out.push_str(name);
                out.push('\t');
                out.push_str(kind);
                out.push('\t');
                out.push_str(&rendered);
                out.push('\n');
            }

            // Atomic write: temp file + rename, so a crash mid-write never
            // leaves a half-written store on disk.
            let tmp = self.path.with_extension("tmp");
            let mut file = fs::File::create(&tmp).map_err(|_| ParamStoreError::Backend)?;
            file.write_all(out.as_bytes())
                .map_err(|_| ParamStoreError::Backend)?;
            file.sync_all().ok();
            fs::rename(&tmp, &self.path).map_err(|_| ParamStoreError::Backend)?;
            Ok(())
        }
    }
}

#[cfg(feature = "std")]
pub use file::FileParamStore;

#[cfg(all(test, feature = "std"))]
mod tests {
    extern crate std;

    use std::{fs, path::PathBuf, vec::Vec};

    use super::*;
    use crate::types::ParameterValue;

    fn temp_path(tag: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(std::format!("nros_param_{tag}_{unique}.store"))
    }

    #[test]
    fn null_store_is_noop() {
        let mut store = NullParamStore;
        let bool_v = ParameterValue::Bool(true);
        let mut iter = [("a", &bool_v)].into_iter();
        assert_eq!(store.save(&mut iter), Ok(()));
        let mut applied = 0;
        store.load(&mut |_, _| applied += 1);
        assert_eq!(applied, 0);
    }

    #[test]
    fn file_store_round_trips_scalars() {
        let path = temp_path("roundtrip");
        let mut store = FileParamStore::new(&path);

        let b = ParameterValue::Bool(true);
        let i = ParameterValue::Integer(-42);
        let d = ParameterValue::Double(3.5);
        let s = ParameterValue::from_string("hello world").unwrap();
        let mut iter = [("flag", &b), ("count", &i), ("gain", &d), ("label", &s)].into_iter();
        store.save(&mut iter).unwrap();

        // A fresh store reading the same file recovers every scalar.
        let reader = FileParamStore::new(&path);
        let mut got: Vec<(std::string::String, ParameterValue)> = Vec::new();
        reader.load(&mut |name, value| got.push((name.into(), value)));

        assert_eq!(got.len(), 4);
        let find = |n: &str| got.iter().find(|(k, _)| k == n).map(|(_, v)| v).unwrap();
        assert_eq!(find("flag").as_bool(), Some(true));
        assert_eq!(find("count").as_integer(), Some(-42));
        assert_eq!(find("gain").as_double(), Some(3.5));
        assert_eq!(find("label").as_string(), Some("hello world"));

        fs::remove_file(&path).ok();
    }

    #[test]
    fn file_store_load_absent_is_empty() {
        let reader = FileParamStore::new(temp_path("absent"));
        let mut applied = 0;
        reader.load(&mut |_, _| applied += 1);
        assert_eq!(applied, 0);
    }

    #[test]
    fn boot_overlay_then_flush_restores_runtime_override() {
        // The full 172.H loop the executor orchestrates, exercised on real
        // types: boot 1 declares defaults, a runtime set + flush persists the
        // override; boot 2 re-declares defaults then loads — the override wins,
        // untouched defaults stay.
        use crate::server::ParameterServer;

        let path = temp_path("e2e");

        // Boot 1: defaults → runtime override → flush (mirrors flush_param_store).
        let mut server = ParameterServer::new();
        server.declare("gain", ParameterValue::Double(1.0));
        server.declare("mode", ParameterValue::Integer(0));
        assert!(!server.take_dirty(), "declares are not runtime changes");
        server.set("gain", ParameterValue::Double(2.5));
        assert!(server.take_dirty());
        let mut store = FileParamStore::new(&path);
        store
            .save(&mut server.iter().map(|p| (p.name(), &p.value)))
            .unwrap();

        // Boot 2: fresh server, defaults, then overlay persisted overrides
        // (mirrors enable_parameter_persistence).
        let mut booted = ParameterServer::new();
        booted.declare("gain", ParameterValue::Double(1.0));
        booted.declare("mode", ParameterValue::Integer(0));
        FileParamStore::new(&path).load(&mut |name, value| {
            let _ = booted.set(name, value);
        });

        assert_eq!(booted.get_double("gain"), Some(2.5), "override restored");
        assert_eq!(booted.get_integer("mode"), Some(0), "default preserved");

        fs::remove_file(&path).ok();
    }

    #[test]
    fn file_store_skips_non_scalar_and_delimiter_corruption() {
        let path = temp_path("skip");
        let mut store = FileParamStore::new(&path);

        let arr = ParameterValue::IntegerArray(Default::default());
        let unset = ParameterValue::NotSet;
        let tabbed = ParameterValue::from_string("a\tb").unwrap();
        let ok = ParameterValue::Integer(7);
        let mut iter = [
            ("arr", &arr),
            ("none", &unset),
            ("bad", &tabbed),
            ("good", &ok),
        ]
        .into_iter();
        store.save(&mut iter).unwrap();

        let mut got = Vec::new();
        FileParamStore::new(&path)
            .load(&mut |name, value| got.push((std::string::String::from(name), value)));
        // Only the clean scalar survives.
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, "good");
        assert_eq!(got[0].1.as_integer(), Some(7));

        fs::remove_file(&path).ok();
    }
}
