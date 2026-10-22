#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

/// List of approved licenses that are compatible with MIT-licensed distribution.
const ALLOWED_LICENSES: &[&str] = &[
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-3-Clause",
    "BSL-1.0",
    "ISC",
    "MIT",
    "OFL-1.1",
    "Ubuntu-font-1.0",
    "Unicode-3.0",
    "Zlib",
];

/// Checks if an individual license identifier is in the allowed list.
fn is_license_allowed(lic: &str) -> bool {
    let trimmed = lic.trim();
    ALLOWED_LICENSES
        .iter()
        .any(|&allowed| allowed.eq_ignore_ascii_case(trimmed))
}

/// Token in a simplified SPDX expression.
#[derive(Debug, PartialEq, Eq)]
enum Token<'a> {
    License(&'a str),
    Or,
    And,
    OpenParen,
    CloseParen,
}

/// Tokenizes an SPDX license expression (supporting AND, OR, parentheses, and legacy '/' syntax).
fn tokenize_spdx(input: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();

    while let Some(&(i, ch)) = chars.peek() {
        match ch {
            ' ' | '\t' | '\r' | '\n' => {
                chars.next();
            }
            '(' => {
                tokens.push(Token::OpenParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::CloseParen);
                chars.next();
            }
            '/' => {
                // Legacy SPDX slash means OR
                tokens.push(Token::Or);
                chars.next();
            }
            _ => {
                let start = i;
                let mut end = i;
                while let Some(&(j, c)) = chars.peek() {
                    if c == '(' || c == ')' || c == '/' || c.is_whitespace() {
                        break;
                    }
                    end = j + c.len_utf8();
                    chars.next();
                }
                let word = &input[start..end];
                if word.eq_ignore_ascii_case("OR") {
                    tokens.push(Token::Or);
                } else if word.eq_ignore_ascii_case("AND") {
                    tokens.push(Token::And);
                } else if word.eq_ignore_ascii_case("WITH") {
                    if let Some(Token::License(prev)) = tokens.pop() {
                        let combined_end = chars.peek().map(|&(idx, _)| idx).unwrap_or(input.len());
                        let full_with =
                            &input[prev.as_ptr() as usize - input.as_ptr() as usize..combined_end];
                        tokens.push(Token::License(full_with.trim()));
                    } else {
                        tokens.push(Token::License(word));
                    }
                } else {
                    tokens.push(Token::License(word));
                }
            }
        }
    }

    tokens
}

/// Evaluates whether an SPDX expression satisfies MIT compatibility.
/// - In `A OR B`, if either A or B is MIT-compatible, downstream users can choose that branch.
/// - In `A AND B`, both A and B must be compatible.
fn eval_spdx_tokens(tokens: &[Token<'_>]) -> bool {
    if tokens.is_empty() {
        return false;
    }

    // Split top-level OR clauses (lowest precedence)
    let mut depth = 0usize;
    let mut or_split_idx = None;

    for (idx, token) in tokens.iter().enumerate() {
        match token {
            Token::OpenParen => depth += 1,
            Token::CloseParen => depth = depth.saturating_sub(1),
            Token::Or if depth == 0 => {
                or_split_idx = Some(idx);
                break;
            }
            _ => {}
        }
    }

    if let Some(idx) = or_split_idx {
        return eval_spdx_tokens(&tokens[..idx]) || eval_spdx_tokens(&tokens[idx + 1..]);
    }

    // Split top-level AND clauses
    let mut and_split_idx = None;
    for (idx, token) in tokens.iter().enumerate() {
        match token {
            Token::OpenParen => depth += 1,
            Token::CloseParen => depth = depth.saturating_sub(1),
            Token::And if depth == 0 => {
                and_split_idx = Some(idx);
                break;
            }
            _ => {}
        }
    }

    if let Some(idx) = and_split_idx {
        return eval_spdx_tokens(&tokens[..idx]) && eval_spdx_tokens(&tokens[idx + 1..]);
    }

    // Strip outer parentheses if present
    if tokens.first() == Some(&Token::OpenParen) && tokens.last() == Some(&Token::CloseParen) {
        let mut inner_depth = 0usize;
        let mut balanced = true;
        for token in &tokens[1..tokens.len() - 1] {
            match token {
                Token::OpenParen => inner_depth += 1,
                Token::CloseParen => {
                    if inner_depth == 0 {
                        balanced = false;
                        break;
                    }
                    inner_depth -= 1;
                }
                _ => {}
            }
        }
        if balanced && inner_depth == 0 {
            return eval_spdx_tokens(&tokens[1..tokens.len() - 1]);
        }
    }

    // Single license identifier
    if tokens.len() == 1 {
        if let Token::License(lic) = tokens[0] {
            return is_license_allowed(lic);
        }
    }

    false
}

/// Evaluates if an expression string is compatible with the MIT license.
fn is_license_compatible_with_mit(expr: &str) -> bool {
    let tokens = tokenize_spdx(expr);
    eval_spdx_tokens(&tokens)
}

#[test]
fn test_license_compatibility_evaluator_unit() {
    // Single allowed licenses
    assert!(is_license_compatible_with_mit("MIT"));
    assert!(is_license_compatible_with_mit("Apache-2.0"));
    assert!(is_license_compatible_with_mit("BSD-3-Clause"));
    assert!(is_license_compatible_with_mit("ISC"));

    // Forbidden / strong or weak copyleft or problematic without alternative
    assert!(!is_license_compatible_with_mit("GPL-3.0-only"));
    assert!(!is_license_compatible_with_mit("AGPL-3.0"));
    assert!(!is_license_compatible_with_mit("MPL-2.0"));
    assert!(!is_license_compatible_with_mit("CC0-1.0"));
    assert!(!is_license_compatible_with_mit("Unlicense"));
    assert!(!is_license_compatible_with_mit("Proprietary"));
    assert!(!is_license_compatible_with_mit(""));

    // Dual licenses with OR: allowed if at least one is compatible
    assert!(is_license_compatible_with_mit("Apache-2.0 OR GPL-2.0-only"));
    assert!(is_license_compatible_with_mit("MIT OR Apache-2.0"));
    assert!(is_license_compatible_with_mit("MIT/Apache-2.0"));
    assert!(is_license_compatible_with_mit("Unlicense OR MIT"));

    // Conjunctions with AND: all must be compatible
    assert!(is_license_compatible_with_mit("Apache-2.0 AND ISC"));
    assert!(!is_license_compatible_with_mit("MIT AND GPL-3.0-only"));

    // Complex nested expressions
    assert!(is_license_compatible_with_mit(
        "(MIT OR Apache-2.0) AND Unicode-3.0"
    ));
}

#[test]
fn test_all_dependencies_have_mit_compatible_licenses() {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .output()
        .expect("cargo metadata execution failed");

    assert!(
        output.status.success(),
        "cargo metadata returned failure status"
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Failed to parse cargo metadata JSON");

    let packages = json["packages"]
        .as_array()
        .expect("Expected packages array in metadata");

    let mut violations = Vec::new();

    for pkg in packages {
        let name = pkg["name"].as_str().unwrap_or("unknown");
        let version = pkg["version"].as_str().unwrap_or("0.0.0");

        // Skip root crate itself
        if name == "cryptomator_rs" || name == "cryptomator-rs" {
            continue;
        }

        let license_opt = pkg["license"].as_str();

        match license_opt {
            Some(lic) => {
                if !is_license_compatible_with_mit(lic) {
                    violations.push(format!(
                        "{name} v{version} has incompatible license: '{lic}'"
                    ));
                }
            }
            None => {
                violations.push(format!(
                    "{name} v{version} has NO license specified (unlicensed)"
                ));
            }
        }
    }

    if !violations.is_empty() {
        let err_msg = format!(
            "\nFound {} dependency crates with incompatible or missing licenses:\n- {}",
            violations.len(),
            violations.join("\n- ")
        );
        panic!("{err_msg}");
    }
}
