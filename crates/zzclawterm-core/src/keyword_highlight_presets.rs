//! Built-in keyword highlight presets (Tauri `keywordHighlightPresets.ts`).
//! Patterns are adapted for the Rust `regex` crate (no look-around).

use crate::models::KeywordHighlightRule;
use std::collections::HashMap;

/// Resolved rule with a single theme-selected color (runtime paint).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKeywordHighlightRule {
    pub id: String,
    pub name: String,
    pub patterns: Vec<String>,
    pub color: String,
    pub enabled: bool,
}

fn token_boundary(alts: &[&str]) -> String {
    // Approximate JS requireTokenBoundary without look-around: word-ish boundaries.
    let inner = alts.join("|");
    format!(r"(?i)(?:^|[^\w-])(?:{inner})(?:$|[^\w-])")
}

/// Built-in rule ids in paint priority order (user rules still take precedence when merged first).
pub fn builtin_keyword_rule_ids() -> &'static [&'static str] {
    &[
        "builtin-url",
        "builtin-version",
        "builtin-address",
        "builtin-size",
        "builtin-string",
        "builtin-option",
        "builtin-uuid",
        "builtin-datetime",
        "builtin-error",
        "builtin-warn",
        "builtin-success",
        "builtin-info",
        "builtin-debug",
        "builtin-duration",
        "builtin-constant",
        "builtin-number",
        "builtin-prompt",
        "builtin-operator",
    ]
}

pub fn builtin_keyword_rule_label(id: &str) -> &'static str {
    match id {
        "builtin-url" => "URL",
        "builtin-version" => "Version",
        "builtin-address" => "Address",
        "builtin-size" => "Size",
        "builtin-string" => "String",
        "builtin-option" => "Option",
        "builtin-uuid" => "UUID",
        "builtin-datetime" => "DateTime",
        "builtin-error" => "Error",
        "builtin-warn" => "Warning",
        "builtin-success" => "Success",
        "builtin-info" => "Info",
        "builtin-debug" => "Debug",
        "builtin-duration" => "Duration",
        "builtin-constant" => "Constant",
        "builtin-number" => "Number",
        "builtin-prompt" => "Prompt",
        "builtin-operator" => "Operator",
        _ => "Builtin",
    }
}

pub fn builtin_keyword_rule_swatch(id: &str, is_dark: bool) -> &'static str {
    let dark = is_dark;
    match id {
        "builtin-error" => {
            if dark {
                "#ff7b72"
            } else {
                "#cf222e"
            }
        }
        "builtin-warn" => {
            if dark {
                "#e3b341"
            } else {
                "#9a6700"
            }
        }
        "builtin-success" => {
            if dark {
                "#3fb950"
            } else {
                "#116329"
            }
        }
        "builtin-info" => {
            if dark {
                "#79c0ff"
            } else {
                "#0969da"
            }
        }
        "builtin-debug" => {
            if dark {
                "#d2a8ff"
            } else {
                "#8250df"
            }
        }
        "builtin-option" => {
            if dark {
                "#ff9e64"
            } else {
                "#b04a00"
            }
        }
        "builtin-datetime" => {
            if dark {
                "#f1fa8c"
            } else {
                "#a58900"
            }
        }
        "builtin-number" => {
            if dark {
                "#bd93f9"
            } else {
                "#6f42c1"
            }
        }
        "builtin-constant" => {
            if dark {
                "#ffb86c"
            } else {
                "#cb4b16"
            }
        }
        "builtin-address" => {
            if dark {
                "#56d364"
            } else {
                "#1a7f37"
            }
        }
        "builtin-url" => {
            if dark {
                "#8be9fd"
            } else {
                "#2aa198"
            }
        }
        "builtin-uuid" => {
            if dark {
                "#ffb86c"
            } else {
                "#bc4c00"
            }
        }
        "builtin-string" => {
            if dark {
                "#f1fa8c"
            } else {
                "#1a8c8c"
            }
        }
        "builtin-operator" => {
            if dark {
                "#7ee787"
            } else {
                "#008573"
            }
        }
        "builtin-version" => {
            if dark {
                "#ff9e64"
            } else {
                "#b04a00"
            }
        }
        "builtin-size" => {
            if dark {
                "#2ac3de"
            } else {
                "#007197"
            }
        }
        "builtin-duration" => {
            if dark {
                "#f1fa8c"
            } else {
                "#859900"
            }
        }
        "builtin-prompt" => {
            if dark {
                "#f92672"
            } else {
                "#e00862"
            }
        }
        _ => {
            if dark {
                "#79c0ff"
            } else {
                "#0969da"
            }
        }
    }
}

fn builtin_patterns(id: &str) -> Vec<String> {
    match id {
        "builtin-url" => vec![r"(?i)\b(?:https?|ftp|wss?)://[-\w+&@#/%?=~_|!:,.;]*[-\w+&@#/%=~_|]".into()],
        "builtin-version" => vec![
            r"(?i)\bv\d+(?:\.\d+){1,2}(?:-[a-z0-9.-]+)?\b".into(),
            r"(?i)\b\d+(?:\.\d+){2,}(?:-[a-z0-9.-]+)?\b".into(),
            r"(?i)\b(?:latest|release|stable|beta|alpha|revision)\b".into(),
        ],
        "builtin-address" => vec![
            r"\b(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\.(?:25[0-5]|2[0-4]\d|[01]?\d\d?)\b".into(),
            r"\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b".into(),
        ],
        "builtin-size" => {
            vec![r"(?i)\b\d+(?:\.\d+)?\s*(?:[kmgtep]i?b|b|bytes?|[kmgtep]bps)\b".into()]
        }
        "builtin-string" => vec![
            r#""(?:[^"\\]|\\.)*""#.into(),
            r"'(?:[^'\\]|\\.)*'".into(),
        ],
        "builtin-option" => vec![
            r"(?i)--[a-zA-Z][\w-]*".into(),
            r"(?i)-[a-zA-Z][a-zA-Z0-9]*".into(),
        ],
        "builtin-uuid" => {
            vec![r"(?i)\b[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}\b".into()]
        }
        "builtin-datetime" => vec![
            r"\b\d{4}[-/]\d{2}[-/]\d{2}(?:T(?:[01]\d|2[0-3])[-:][0-5]\d[-:][0-5]\d(?:\.\d{1,9})?(?:Z|[+-]\d{2}:?\d{2})?)?\b".into(),
            r"\b(?:[01]\d|2[0-3]):[0-5]\d(?::[0-5]\d)?(?:\.\d{1,9})?\b".into(),
        ],
        "builtin-error" => vec![token_boundary(&[
            "access denied",
            "address already in use",
            "authentication failed",
            "broken pipe",
            "cannot allocate memory",
            "command not found",
            "connection refused",
            "connection reset by peer",
            "connection timed out",
            "error",
            "fail(?:ed|ure)?",
            "fatal",
            "exception",
            "host key verification failed",
            "module not found",
            "network is unreachable",
            "no route to host",
            "no space left on device",
            "operation timed out",
            "out of memory",
            "panic",
            "permission denied",
            "port already in use",
            "segmentation fault",
            "critical",
            "traceback",
            "unable to connect",
        ])],
        "builtin-warn" => vec![token_boundary(&["warn(?:ing)?", "deprecated", "caution"])],
        "builtin-success" => vec![token_boundary(&[
            "accepted",
            "active",
            "already up to date",
            "authenticated",
            "authorized",
            "available",
            "backup completed",
            "completed successfully",
            "deployed",
            "enabled",
            "installed",
            "listening",
            "login successful",
            "online",
            "operation completed",
            "reachable",
            "restored",
            "synchronized",
            "synced",
            "updated",
            "upgraded",
            "validated",
            "verified",
            "success(?:ful(?:ly)?)?",
            "ok",
            "done",
            "pass(?:ed)?",
            "complet(?:e|ed)",
            "ready",
            "healthy",
            "running",
            "started",
            "connected",
            "uploaded",
            "downloaded",
            "created",
            "saved",
            "finished",
        ])],
        "builtin-info" => vec![token_boundary(&["info(?:rmation)?", "notice"])],
        "builtin-debug" => vec![token_boundary(&["debug", "trace", "verbose"])],
        "builtin-duration" => vec![
            r"(?i)\b[-+]?\d+(?:\.\d+)?\s*(?:ns|us|µs|ms|sec|mins?|minutes|hrs?|hours|days|weeks|months|years)\b".into(),
        ],
        "builtin-constant" => vec![token_boundary(&[
            "true",
            "false",
            "null",
            "nil",
            "none",
            "undefined",
            "NaN",
            "Infinity",
            "nullptr",
            "EOF",
            "stop(?:ped)?",
            "exit(?:ed|ing)?",
            "quit(?:ed|ing)?",
            "abort(?:ed|ing)?",
            "cancel(?:ed|ing)?",
            "interrupt(?:ed|ing)?",
            "pause(?:ed|ing)?",
            "resume(?:ed|ing)?",
        ])],
        "builtin-number" => vec![
            r"(?i)\b[-+]?0x[0-9a-f]+\b".into(),
            r"(?i)\b[-+]?(?:\.\d+|\d+\.\d*|\d+[eE][-+]?\d+|\d{2,})(?:[eE][-+]?\d+)?(?:\s*%)?\b".into(),
        ],
        "builtin-prompt" => vec![r"[$#](?=\s)".into()],
        "builtin-operator" => vec![r"[-:=+&*()$\[\]<>?|{}]+".into()],
        _ => Vec::new(),
    }
}

/// Built-in rules coloured for the current terminal theme family.
pub fn get_builtin_keyword_rules(is_dark: bool) -> Vec<ResolvedKeywordHighlightRule> {
    builtin_keyword_rule_ids()
        .iter()
        .map(|id| ResolvedKeywordHighlightRule {
            id: (*id).to_string(),
            name: builtin_keyword_rule_label(id).to_string(),
            patterns: builtin_patterns(id),
            color: builtin_keyword_rule_swatch(id, is_dark).to_string(),
            enabled: true,
        })
        .collect()
}

/// Merge user rules (higher priority) then enabled built-ins for paint.
pub fn merge_keyword_highlight_rules_for_paint(
    user_rules: &[KeywordHighlightRule],
    builtin_settings: &HashMap<String, bool>,
    is_dark: bool,
) -> Vec<ResolvedKeywordHighlightRule> {
    let mut out = Vec::new();
    for rule in user_rules {
        out.push(ResolvedKeywordHighlightRule {
            id: rule.id.clone(),
            name: rule.name.clone(),
            patterns: rule.patterns.clone(),
            color: if is_dark {
                rule.color_dark.clone()
            } else {
                rule.color_light.clone()
            },
            enabled: rule.enabled,
        });
    }
    for builtin in get_builtin_keyword_rules(is_dark) {
        let enabled = builtin_settings.get(&builtin.id).copied().unwrap_or(true);
        out.push(ResolvedKeywordHighlightRule { enabled, ..builtin });
    }
    out
}

/// Curated swatches for rule color pickers (Tauri DARK_PALETTE / LIGHT_PALETTE subset).
pub fn keyword_highlight_color_palette(is_dark: bool) -> &'static [&'static str] {
    if is_dark {
        &[
            "#ff7b72", "#e3b341", "#3fb950", "#79c0ff", "#d2a8ff", "#ff9e64", "#f1fa8c", "#bd93f9",
            "#ffb86c", "#56d364", "#8be9fd", "#7ee787",
        ]
    } else {
        &[
            "#cf222e", "#9a6700", "#116329", "#0969da", "#8250df", "#b04a00", "#a58900", "#6f42c1",
            "#cb4b16", "#1a7f37", "#2aa198", "#008573",
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::{
        KeywordHighlightRule, get_builtin_keyword_rules, merge_keyword_highlight_rules_for_paint,
    };
    use std::collections::HashMap;

    #[test]
    fn builtin_catalog_is_stable() {
        let rules = get_builtin_keyword_rules(true);
        assert_eq!(rules.len(), 18);
        assert!(rules.iter().all(|rule| rule.id.starts_with("builtin-")));
        assert!(!rules[0].patterns.is_empty());
    }

    #[test]
    fn merge_puts_user_before_builtin() {
        let user = vec![KeywordHighlightRule {
            id: "kh-1".into(),
            name: "Panic".into(),
            patterns: vec!["panic".into()],
            color_dark: "#ff0000".into(),
            color_light: "#aa0000".into(),
            enabled: true,
        }];
        let mut settings = HashMap::new();
        settings.insert("builtin-error".into(), false);
        let merged = merge_keyword_highlight_rules_for_paint(&user, &settings, true);
        assert_eq!(merged[0].id, "kh-1");
        assert_eq!(merged[0].color, "#ff0000");
        let error = merged.iter().find(|r| r.id == "builtin-error").unwrap();
        assert!(!error.enabled);
    }
}
