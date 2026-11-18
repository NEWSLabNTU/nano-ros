// Issue 2638.
//! The TEXT an embedded image is baked with for one launch parameter value.
//!
//! An image has no launch runtime to hand `--params-file` to, so every
//! resolved parameter is baked as text and typed again on the device when it
//! seeds the store (`nros_params::ParameterValue::assign_launch_text`). This
//! is the writer of that convention and the device side is its reader; the
//! round trip is tested here against the reader itself.
//!
//! It wraps `ParamValue::to_bake_string` (rlm) for scalars and differs in two
//! places, both where the reader would otherwise type the text as something
//! the model did not say:
//!
//! - a LIST is written as a YAML flow sequence, `[40.0, 50.0, 60.0]`, its
//!   elements spelled as the model spells them (a string that would re-type
//!   is already single-quoted there). `to_bake_string` joins with `,`
//!   (`40.0,50.0,60.0`), which reads back as one STRING -- and a one-element
//!   list as a scalar, an empty one as `""`. That is how an MPC rate-limit
//!   list `[40.0, 50.0, 60.0]` from a `config/*.param.yaml` reached the node
//!   as a string parameter.
//! - a STRING whose text would read as a LIST or as a quoted string (`[a]`,
//!   `'q'`) is single-quoted, `'` doubled, which the reader reads back as
//!   exactly that string. Those two spellings meant a string before lists
//!   were read, and must keep meaning one.
//!
//! A string that reads as a number or a bool (`"50"`) is written as is, and
//! the device infers it, as it always has. A version-1 model carries launch
//! text in `Str` (`rate: "50"` is the integer 50 there), so quoting it would
//! turn such a model's integers into strings.

use ros_launch_manifest_model::{NodeInstance, ParamValue};

/// Every launch parameter of the node `fqn` names, as the text an image bakes:
/// `params_files` projected under the inline `<param>` values
/// (`NodeInstance::resolved_params`), each value through [`launch_param_text`].
/// Sorted by name.
///
/// The ONE projection from a model's node to its launch values. A firmware
/// entry (`codegen entry`, `nros::main!`), the planner's record and the
/// metadata probe's seeds all read it, so a value the probe constructs a node
/// with is the value the image is baked with. `qos_overrides.*` keys are
/// included; each consumer splits them off (`qos_override::is_qos_override`).
pub fn baked_params(inst: &NodeInstance, fqn: &str) -> Vec<(String, String)> {
    // Issue 2640 -- was spelled three times (entry, macro, plan record).
    inst.resolved_params(fqn)
        .iter()
        .map(|(k, v)| (k.clone(), launch_param_text(v)))
        .collect()
}

/// The executable a model's node instance names: its `exec`, else the last
/// segment of its composable `plugin` (`pkg::Class` -> `Class`). With the
/// node's `pkg`, the key a component's source metadata is matched on.
pub fn instance_executable(inst: &NodeInstance) -> Option<String> {
    inst.exec.clone().or_else(|| {
        inst.plugin
            .as_deref()
            .map(|p| p.rsplit("::").next().unwrap_or(p).to_string())
    })
}

/// The baked text for `value`. See the module docs.
pub fn launch_param_text(value: &ParamValue) -> String {
    match value {
        ParamValue::StrList(items) => format!("[{}]", items.join(", ")),
        ParamValue::Str(s) if string_needs_quotes(s) => format!("'{}'", s.replace('\'', "''")),
        other => other.to_bake_string(),
    }
}

/// Would the reader read `s`, written bare, as a list or a quoted string?
fn string_needs_quotes(s: &str) -> bool {
    let t = s.trim();
    (t.starts_with('[') && t.ends_with(']'))
        || (s.len() >= 2 && s.starts_with('\'') && s.ends_with('\''))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nros_params::ParameterValue as Device;

    fn list(items: &[&str]) -> ParamValue {
        ParamValue::StrList(items.iter().map(|s| s.to_string()).collect())
    }

    fn device(text: &str) -> Device {
        let mut v = Device::NotSet;
        v.assign_launch_text(text)
            .unwrap_or_else(|e| panic!("`{text}` has no value: {e:?}"));
        v
    }

    #[test]
    fn a_double_list_arrives_as_a_double_array() {
        let text = launch_param_text(&list(&["40.0", "50.0", "60.0"]));
        assert_eq!(text, "[40.0, 50.0, 60.0]");
        match device(&text) {
            Device::DoubleArray(v) => assert_eq!(v.as_slice(), &[40.0, 50.0, 60.0]),
            other => panic!("{other:?}"),
        }
        match device(&launch_param_text(&list(&["1.5"]))) {
            Device::DoubleArray(v) => assert_eq!(v.as_slice(), &[1.5]),
            other => panic!("one element is still a list: {other:?}"),
        }
    }

    #[test]
    fn integer_bool_and_string_lists_keep_their_element_type() {
        match device(&launch_param_text(&list(&["1", "2", "3"]))) {
            Device::IntegerArray(v) => assert_eq!(v.as_slice(), &[1, 2, 3]),
            other => panic!("{other:?}"),
        }
        match device(&launch_param_text(&list(&["true", "false"]))) {
            Device::BoolArray(v) => assert_eq!(v.as_slice(), &[true, false]),
            other => panic!("{other:?}"),
        }
        // The model single-quotes an element that would re-type.
        match device(&launch_param_text(&list(&["a", "'5'", "'x, y'"]))) {
            Device::StringArray(v) => {
                let got: Vec<&str> = v.iter().map(|s| s.as_str()).collect();
                assert_eq!(got, ["a", "5", "x, y"]);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_empty_list_bakes_as_one_and_seeds_nothing() {
        let text = launch_param_text(&list(&[]));
        assert_eq!(text, "[]");
        let mut v = Device::NotSet;
        assert_eq!(
            v.assign_launch_text(&text),
            Err(nros_params::LaunchTextError::EmptyList)
        );
    }

    #[test]
    fn scalars_round_trip() {
        assert!(
            matches!(device(&launch_param_text(&ParamValue::Float(1.0))), Device::Double(d) if d == 1.0)
        );
        assert!(matches!(
            device(&launch_param_text(&ParamValue::Int(7))),
            Device::Integer(7)
        ));
        assert!(matches!(
            device(&launch_param_text(&ParamValue::Bool(true))),
            Device::Bool(true)
        ));
        // A number- or bool-looking string keeps its inference (version-1
        // models carry launch text in `Str`).
        assert!(matches!(
            device(&launch_param_text(&ParamValue::Str("50".into()))),
            Device::Integer(50)
        ));
        for s in ["frame", "", "[a, b]", "'q'", "it's", "40.0,50.0"] {
            let text = launch_param_text(&ParamValue::Str(s.into()));
            match device(&text) {
                Device::String(got) => assert_eq!(got.as_str(), s, "baked as `{text}`"),
                other => panic!("string `{s}` baked as `{text}` read back as {other:?}"),
            }
        }
    }
}
