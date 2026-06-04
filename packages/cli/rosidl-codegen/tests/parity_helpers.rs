// Utilities for comparing generated code with reference outputs from rosidl_generator_rs
use similar::{ChangeTag, TextDiff};

/// Normalize whitespace in code for comparison
/// - Trims leading/trailing whitespace from each line
/// - Removes empty lines
/// - Normalizes to single newlines between content
pub fn normalize_whitespace(code: &str) -> String {
    code.lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Strip single-line comments from code
/// Removes lines starting with // or ///
pub fn strip_comments(code: &str) -> String {
    code.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !trimmed.starts_with("//") && !trimmed.starts_with("///")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Normalize package paths for comparison
/// Converts `package::msg::Type` to `crate::msg::Type` for local types
/// This handles differences in how rosidl_generator_rs and our codegen refer to types
pub fn normalize_paths(code: &str, package_name: &str) -> String {
    code.replace(&format!("{}::", package_name), "crate::")
        .replace(&format!("::{}", package_name), "::crate")
}

/// Normalize use statements
/// Sorts use statements and removes duplicates for easier comparison
pub fn normalize_use_statements(code: &str) -> String {
    let mut lines = Vec::new();
    let mut use_statements = Vec::new();
    let mut in_use_block = false;

    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("use ") {
            use_statements.push(line.to_string());
            in_use_block = true;
        } else if in_use_block && trimmed.is_empty() {
            // End of use block - sort and add
            use_statements.sort();
            use_statements.dedup();
            lines.append(&mut use_statements);
            lines.push(line.to_string());
            in_use_block = false;
        } else {
            if in_use_block && !trimmed.is_empty() {
                // Flush use statements before non-use content
                use_statements.sort();
                use_statements.dedup();
                lines.append(&mut use_statements);
                in_use_block = false;
            }
            lines.push(line.to_string());
        }
    }

    // Flush any remaining use statements
    if !use_statements.is_empty() {
        use_statements.sort();
        use_statements.dedup();
        lines.extend(use_statements);
    }

    lines.join("\n")
}

/// Apply all normalization steps for comparison
pub fn normalize_code(code: &str, package_name: &str) -> String {
    let mut normalized = code.to_string();
    normalized = strip_comments(&normalized);
    normalized = normalize_paths(&normalized, package_name);
    normalized = normalize_use_statements(&normalized);
    normalized = normalize_whitespace(&normalized);
    normalized
}

/// Print a colored diff between two code strings
/// Returns true if codes are identical, false otherwise
pub fn print_diff(label_ours: &str, label_reference: &str, ours: &str, reference: &str) -> bool {
    if ours == reference {
        return true;
    }

    println!("\n❌ diff between {} and {}:", label_ours, label_reference);
    println!("{}", "=".repeat(80));

    let diff = TextDiff::from_lines(reference, ours);

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal => " ",
        };
        print!("{}{}", sign, change);
    }

    println!("{}", "=".repeat(80));

    // Print statistics
    let stats = diff
        .ops()
        .iter()
        .fold((0, 0, 0), |(del, ins, eq), op| match op {
            similar::DiffOp::Delete { old_len, .. } => (del + old_len, ins, eq),
            similar::DiffOp::Insert { new_len, .. } => (del, ins + new_len, eq),
            similar::DiffOp::Equal { len, .. } => (del, ins, eq + len),
            _ => (del, ins, eq),
        });

    println!(
        "Lines: {} deleted, {} inserted, {} equal",
        stats.0, stats.1, stats.2
    );

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_whitespace() {
        let code = "  fn foo()  \n\n  {  \n    bar();\n  }  ";
        let normalized = normalize_whitespace(code);
        assert_eq!(normalized, "  fn foo()\n\n  {\n    bar();\n  }");
    }

    #[test]
    fn test_strip_comments() {
        let code = r#"
// This is a comment
fn foo() {
    /// Doc comment
    bar(); // inline comment (kept)
}
"#;
        let stripped = strip_comments(code);
        assert!(!stripped.contains("// This is a comment"));
        assert!(!stripped.contains("/// Doc comment"));
        assert!(stripped.contains("bar(); // inline comment"));
    }

    #[test]
    fn test_normalize_paths() {
        let code = "std_msgs::msg::String";
        let normalized = normalize_paths(code, "std_msgs");
        assert_eq!(normalized, "crate::msg::String");
    }

    #[test]
    fn test_normalize_use_statements() {
        let code = r#"
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use std::string::String;

fn foo() {}
"#;
        let normalized = normalize_use_statements(code);
        let lines: Vec<&str> = normalized.lines().collect();

        // Use statements should be sorted
        assert!(lines[1].contains("serde"));
        assert!(lines[2].contains("std::collections"));
        assert!(lines[3].contains("std::string"));
    }

    #[test]
    fn test_print_diff_identical() {
        let code1 = "fn foo() {}";
        let code2 = "fn foo() {}";
        assert!(print_diff("ours", "reference", code1, code2));
    }

    #[test]
    fn test_print_diff_different() {
        let code1 = "fn foo() {}";
        let code2 = "fn bar() {}";
        assert!(!print_diff("ours", "reference", code1, code2));
    }
}
