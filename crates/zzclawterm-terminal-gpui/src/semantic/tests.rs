use std::sync::Arc;

use zzclawterm_core::keyword_highlight_presets::{
    builtin_keyword_rule_swatch, get_builtin_keyword_rules,
};
use zzclawterm_terminal::{ShellInputLineKind, TerminalScreen, TerminalSnapshot};

use super::{SemanticContext, SemanticHighlighter};
use crate::keywords::{
    compile_terminal_keyword_highlighter, parse_hex_rgb, precompute_terminal_keyword_highlights,
    precompute_terminal_keyword_highlights_for_rows_with_stats,
    precompute_terminal_keyword_highlights_for_rows_with_stats_and_cancel,
};
use crate::paint::terminal_highlight_spans_with_keyword_ranges_and_options;
use crate::types::TerminalKeywordRange;

fn swatch(id: &str) -> u32 {
    parse_hex_rgb(builtin_keyword_rule_swatch(id, true)).unwrap()
}

fn colors(line: &str, input: bool) -> Vec<Option<u32>> {
    let highlighter = SemanticHighlighter::compile(&get_builtin_keyword_rules(true));
    let context = SemanticContext {
        integrated: true,
        input: if input {
            std::iter::once(0..line.len()).collect()
        } else {
            Vec::new()
        },
        ..SemanticContext::default()
    };
    let mut colors = vec![None; line.len()];
    for (start, end, color) in highlighter.ranges(line, &context, &[]) {
        colors[start..end].fill(Some(color));
    }
    colors
}

fn assert_token(line: &str, token: &str, id: &str, input: bool) {
    let start = line.find(token).unwrap();
    assert!(
        colors(line, input)[start..start + token.len()]
            .iter()
            .all(|color| *color == Some(swatch(id))),
        "{line:?}: {token:?} should be {id}"
    );
}

#[test]
fn docker_names_do_not_highlight_embedded_options_or_hyphens() {
    for name in [
        "zzclawterm-log-ticker",
        "macos-monterey",
        "kanghub-frontend-1",
        "kanghub-redis-1",
        "prefix--flag",
        "界面-name",
    ] {
        assert!(colors(name, false).iter().all(Option::is_none), "{name}");
    }
}

#[test]
fn output_options_require_a_standalone_start_and_keep_help_flags() {
    let cases: &[(&str, &[&str])] = &[
        (
            "Sort entries alphabetically if none of -cftuvSUX nor --sort is specified.",
            &["-cftuvSUX", "--sort"],
        ),
        (
            "  -a, --all  do not ignore entries starting with .",
            &["-a", "--all"],
        ),
        (
            "  -A, --almost-all  do not list implied . and ..",
            &["-A", "--almost-all"],
        ),
        (
            "  --author  with -l, print the author of each file",
            &["--author", "-l"],
        ),
        ("  -b, --escape  print C-style escapes", &["-b", "--escape"]),
        ("[-a|--all]", &["-a", "--all"]),
        ("--color=always", &["--color"]),
        (" -single-dash-option ", &["-single-dash-option"]),
    ];
    for (line, flags) in cases {
        for flag in *flags {
            assert_token(line, flag, "builtin-option", false);
        }
    }
    let rules: Vec<_> = get_builtin_keyword_rules(true)
        .into_iter()
        .filter(|r| r.id == "builtin-option")
        .collect();
    let options = SemanticHighlighter::compile(&rules);
    for text in ["foo-bar", "foo--bar", "file/-flag", "key=-flag", "(--flag)"] {
        assert!(
            options
                .matches(text, &SemanticContext::default())
                .is_empty(),
            "{text}"
        );
    }
    assert_token("a - b", "-", "builtin-operator", false);
    let mut rules = get_builtin_keyword_rules(true);
    rules
        .iter_mut()
        .find(|r| r.id == "builtin-option")
        .unwrap()
        .enabled = false;
    let disabled = SemanticHighlighter::compile(&rules);
    let matches = disabled.ranges("--all", &SemanticContext::default(), &[]);
    assert_eq!(matches, vec![(0, 2, swatch("builtin-operator"))]);
}

#[test]
fn output_option_boundaries_use_logical_lines_across_soft_wraps() {
    let mut screen = TerminalScreen::new(8, 8);
    screen.advance(b"zzclawterm-log-ticker\r\n-cftuvSUX --all");
    let result = snapshot_colors(&screen.snapshot());
    assert!(result[..18].iter().all(Option::is_none));
    assert!(
        result[24..33]
            .iter()
            .all(|color| *color == Some(swatch("builtin-option")))
    );
    assert!(
        result[34..39]
            .iter()
            .all(|color| *color == Some(swatch("builtin-option")))
    );
}

#[test]
fn permission_tokens_are_validated_before_character_decoration() {
    for token in [
        "drwxr-xr-x",
        "-rw-r--r--",
        "lrwxrwxrwx",
        "drwxr-xr-x+",
        "drwxr-xr-x.",
        "drwxrwxrwt",
        "drwxrwxrwT",
        "-rwsr-xr-x",
        "-rwSr-Sr-x",
    ] {
        let line = format!("prefix {token} 18 root root");
        let result = colors(&line, false);
        for (index, byte) in token.bytes().enumerate() {
            let id = match byte {
                b'-' | b'.' | b'+' => "builtin-operator",
                _ if index == 0 => "builtin-debug",
                b'r' => "builtin-info",
                b'w' => "builtin-warn",
                _ => "builtin-error",
            };
            assert_eq!(
                result[7 + index],
                Some(swatch(id)),
                "{token}, column {index}"
            );
        }
    }
    for invalid in [
        "rwx",
        "adrwxr-xr-x",
        "drwxr-xr-xsuffix",
        "drwxr-xr-x,",
        "dwwxr-xr-x",
    ] {
        assert!(
            !colors(invalid, false).contains(&Some(swatch("builtin-debug"))),
            "{invalid}"
        );
    }
}

#[test]
fn listing_datetime_and_numbers_keep_windterm_colors_without_single_digit_noise() {
    let line = "drwxr-xr-x 18 root root 4096 Oct 7 21:37 .git";
    for number in ["18", "4096"] {
        assert_token(line, number, "builtin-number", false);
    }
    for date in ["Oct", "21:37"] {
        assert_token(line, date, "builtin-datetime", false);
    }
    assert_eq!(colors(line, false)[line.find(" 7 ").unwrap() + 1], None);
}

#[test]
fn shell_commands_separators_assignments_paths_and_options_are_contextual() {
    let line = "LANG=C cd /tmp && ls -la | grep error; cat ./Cargo.toml || vim ~/.ssh/config";
    for command in ["cd", "ls", "grep", "cat", "vim"] {
        assert_token(line, command, "builtin-command", true);
    }
    for path in ["/tmp", "./Cargo.toml", "~/.ssh/config"] {
        assert_token(line, path, "builtin-path", true);
    }
    assert_token(line, "-la", "builtin-option", true);
    assert_token("./app /tmp/2026-10-07", "./app", "builtin-command", true);
    assert_token(
        "./app /tmp/2026-10-07",
        "/tmp/2026-10-07",
        "builtin-path",
        true,
    );
    assert_token("echo true", "true", "builtin-constant", true);
    let start = line.find("error").unwrap();
    assert!(
        colors(line, true)[start..start + 5]
            .iter()
            .all(Option::is_none)
    );
    assert!(
        !colors("ordinary output /tmp ./Cargo.toml", false).contains(&Some(swatch("builtin-path")))
    );
}

#[test]
fn urls_specialized_tokens_and_option_values_beat_generic_punctuation_and_numbers() {
    let input = "curl --connect-timeout=5 --color=always https://example.com/a/b";
    assert_token(input, "curl", "builtin-command", true);
    assert_token(input, "--connect-timeout", "builtin-option", true);
    assert_token(input, "5", "builtin-info", true);
    assert_token(input, "always", "builtin-info", true);
    assert_token(input, "https://example.com/a/b", "builtin-url", true);
    for (token, id) in [
        ("192.168.1.1", "builtin-address"),
        ("128MiB", "builtin-size"),
        ("15ms", "builtin-duration"),
        ("v2.7.0", "builtin-version"),
        ("550e8400-e29b-41d4-a716-446655440000", "builtin-uuid"),
    ] {
        assert_token(token, token, id, false);
    }
}

#[test]
fn ipv6_candidates_require_rust_address_validation() {
    let rules: Vec<_> = get_builtin_keyword_rules(true)
        .into_iter()
        .filter(|r| r.id == "builtin-address")
        .collect();
    let addresses = SemanticHighlighter::compile(&rules);
    for token in [
        "::",
        "::1",
        "2001:db8::1",
        "fe80::1234",
        "2001:db8:0:0:0:0:2:1",
        "::ffff:192.168.1.1",
    ] {
        assert_token(token, token, "builtin-address", false);
    }
    for invalid in [
        "21:37",
        "88:77",
        "foo:bar:baz",
        "2001:db8:::1",
        "2001:db8::gg",
        "12345::1",
    ] {
        assert!(
            addresses
                .matches(invalid, &SemanticContext::default())
                .is_empty(),
            "{invalid}"
        );
    }
    assert_token("[2001:db8::1]", "2001:db8::1", "builtin-address", false);
}

#[test]
fn datetime_formats_preserve_valid_time_ranges() {
    let rules: Vec<_> = get_builtin_keyword_rules(true)
        .into_iter()
        .filter(|r| r.id == "builtin-datetime")
        .collect();
    let dates = SemanticHighlighter::compile(&rules);
    for token in [
        "2026-10-07",
        "2026/10/07",
        "10/7/26",
        "10-7-26",
        "10/07/2026",
        "20261007T221600Z",
        "21:37",
        "21:37:59",
        "2026-10-07T21:37:59+08:00",
        "Oct",
        "Mon",
        "Sept",
        "Tues",
        "Thu",
        "Thur",
    ] {
        assert_token(token, token, "builtin-datetime", false);
    }
    // Isolate the category: operators deliberately share DateTime's green swatch.
    for invalid in ["88:77", "24:00", "21:60", "20261307T221600Z"] {
        assert!(
            dates
                .matches(invalid, &SemanticContext::default())
                .is_empty(),
            "{invalid}"
        );
    }
}

#[test]
fn shell_strings_suppress_output_keywords_and_support_unclosed_quotes() {
    for text in [
        "echo \"hello world\"",
        "echo 'ERROR connection refused'",
        "echo \"unterminated",
    ] {
        let start = text.find(['\'', '"']).unwrap();
        assert!(
            colors(text, true)[start..]
                .iter()
                .all(|color| *color == Some(swatch("builtin-string")))
        );
    }
    let line = r#"printf "hello\n%s %02d %.2f %% \x41 \u0041""#;
    for child in [r"\n", "%s", "%02d", "%.2f", "%%", r"\x41", r"\u0041"] {
        assert_token(line, child, "builtin-string", true);
    }
    assert_token(line, "hello", "builtin-string", true);
    assert_token(r"echo 'literal\n'", r"\n", "builtin-string", true);
}

#[test]
fn output_word_boundaries_leave_delimiters_uncolored_by_status() {
    assert_token("[ERROR][ERROR]", "ERROR", "builtin-error", false);
    let result = colors("[ERROR][ERROR]", false);
    assert_ne!(result[0], Some(swatch("builtin-error")));
    assert_ne!(result[6], Some(swatch("builtin-error")));
    assert_eq!(result[8], Some(swatch("builtin-error")));
    for invalid in ["error-prone", "myERROR", "ERROR日志", "error\u{301}"] {
        assert!(!colors(invalid, false).contains(&Some(swatch("builtin-error"))));
    }
    assert_token(
        "ERROR: connection refused",
        "connection refused",
        "builtin-error",
        false,
    );
    assert_token("warning debug success", "warning", "builtin-warn", false);
    assert_token("warning debug success", "debug", "builtin-debug", false);
    assert_token("warning debug success", "success", "builtin-success", false);
}

fn snapshot_colors(snapshot: &TerminalSnapshot) -> Vec<Option<u32>> {
    let highlighter = compile_terminal_keyword_highlighter(&get_builtin_keyword_rules(true));
    let highlights = precompute_terminal_keyword_highlights(
        snapshot,
        &highlighter,
        zzclawterm_ui::theme_palette("github-dark"),
        None,
    );
    let mut result = Vec::new();
    for row in 0..snapshot.row_count() {
        let mut colors = vec![None; snapshot.cols];
        if let Some(lookup) = highlights.lookup(row, snapshot)
            && let Some(ranges) = lookup.ranges()
        {
            for range in ranges {
                colors[range.start_col..range.end_col].fill(Some(range.color));
            }
        }
        result.extend(colors);
    }
    result
}

#[test]
fn osc133_regions_survive_submission_and_split_same_row_output() {
    let mut screen = TerminalScreen::new(80, 2);
    screen.advance(b"\x1b]133;A\x07root@host:/tmp# \x1b]133;B\x07ls -la");
    let input = screen.snapshot();
    let colors = snapshot_colors(&input);
    assert_eq!(colors[14], Some(swatch("builtin-prompt")));
    assert_eq!(&colors[16..18], &[Some(swatch("builtin-command")); 2]);
    screen.advance(b"\x1b]133;C\x07 ERROR: connection refused");
    let submitted = screen.snapshot();
    assert_eq!(
        submitted.row(0).unwrap().shell_input,
        Some(ShellInputLineKind::Submitted)
    );
    let colors = snapshot_colors(&submitted);
    assert_eq!(&colors[16..18], &[Some(swatch("builtin-command")); 2]);
    assert_eq!(&colors[23..28], &[Some(swatch("builtin-error")); 5]);
}

#[test]
fn readline_completion_pages_are_output_while_osc133_input_remains_active() {
    let mut screen = TerminalScreen::new(80, 4);
    screen.advance(b"\x1b]133;A\x07$ \x1b]133;B\x07ls ");
    let input = screen.snapshot();
    assert_eq!(snapshot_colors(&input)[2], Some(swatch("builtin-command")));
    // Readline prints completions and its pager without executing a command,
    // so it does not send OSC 133 C before these hard line breaks.
    screen.advance(b"\r\n01-javascript/\r\n10:58 test/\r\nAGENTS.md\r\n.bashrc\r\n--More--");
    let page = screen.snapshot();
    assert!(page.scrollback_len > 0);
    let page_colors = snapshot_colors(&page);
    assert!(!page_colors.contains(&Some(swatch("builtin-command"))));
    assert_eq!(&page_colors[..5], &[Some(swatch("builtin-datetime")); 5]);
    assert_eq!(page_colors[3 * 80], Some(swatch("builtin-option")));
    let agents = page
        .rows()
        .iter()
        .position(|row| row.text == "AGENTS.md")
        .unwrap();
    assert!(
        page_colors[agents * 80..agents * 80 + 9]
            .iter()
            .all(Option::is_none)
    );
    // Redisplay establishes a fresh input anchor; the candidate list remains output.
    screen.advance(b"\r\x1b[2K$ \x1b]133;B\x07ls ");
    let restored = screen.snapshot();
    let row = restored
        .rows()
        .iter()
        .position(|row| row.text == "$ ls")
        .unwrap();
    assert_eq!(
        snapshot_colors(&restored)[row * 80 + 2],
        Some(swatch("builtin-command"))
    );
    assert!(
        restored.rows()[..row]
            .iter()
            .all(|row| row.shell_input_columns.is_none())
    );
}

#[test]
fn fallback_is_isolated_from_integrated_output_and_soft_wrapped_input() {
    let mut screen = TerminalScreen::new(20, 4);
    screen.advance(b"$ cat file | grep error");
    assert_eq!(
        snapshot_colors(&screen.snapshot())[2],
        Some(swatch("builtin-command"))
    );
    let mut screen = TerminalScreen::new(20, 4);
    screen.advance(b"\x1b]133;A\x07$ \x1b]133;B\x07cat file | grep error");
    let colors = snapshot_colors(&screen.snapshot());
    assert_eq!(colors[13], Some(swatch("builtin-command")));
    assert!(colors[18..23].iter().all(Option::is_none));
    screen.advance(b"\r\n\x1b]133;C\x07$ echo ERROR");
    let snapshot = screen.snapshot();
    let colors = snapshot_colors(&snapshot);
    let output_row = snapshot
        .rows()
        .iter()
        .position(|row| row.text == "$ echo ERROR")
        .unwrap();
    assert_eq!(colors[output_row * 20 + 2], None);
    assert_eq!(colors[output_row * 20 + 7], Some(swatch("builtin-error")));
    screen.advance(b"\x1b[?1049h$ echo ERROR");
    let alternate = screen.snapshot();
    assert!(alternate.rows().iter().all(|row| row.shell_integration));
    assert_eq!(snapshot_colors(&alternate)[2], None);
}

#[test]
fn whole_strings_survive_soft_wrap_and_precompute_cancellation_does_not_publish() {
    let mut screen = TerminalScreen::new(12, 5);
    screen.advance(b"\x1b]133;B\x07echo \"hello\\n%s\"");
    let snapshot = screen.snapshot();
    let colors = snapshot_colors(&snapshot);
    assert_eq!(colors[5], Some(swatch("builtin-string")));
    assert_eq!(&colors[11..13], &[Some(swatch("builtin-string")); 2]);
    assert_eq!(&colors[13..15], &[Some(swatch("builtin-string")); 2]);
    let highlighter = compile_terminal_keyword_highlighter(&get_builtin_keyword_rules(true));
    let mut groups = 0;
    let cancelled = precompute_terminal_keyword_highlights_for_rows_with_stats_and_cancel(
        &snapshot,
        &highlighter,
        zzclawterm_ui::theme_palette("github-dark"),
        None,
        0..snapshot.row_count(),
        || {
            groups += 1;
            groups > 1
        },
    );
    assert!(cancelled.is_none());
}

#[test]
fn unchanged_text_semantic_changes_invalidate_reuse_and_stale_results() {
    let palette = zzclawterm_ui::theme_palette("github-dark");
    let highlighter = compile_terminal_keyword_highlighter(&get_builtin_keyword_rules(true));
    let mut screen = TerminalScreen::new(30, 2);
    screen.advance(b"ERROR");
    let output = screen.snapshot();
    let previous = precompute_terminal_keyword_highlights(&output, &highlighter, palette, None);
    // Move to column zero, then mark the existing text as shell input without changing it.
    screen.advance(b"\r\x1b]133;B\x07");
    let input = screen.snapshot();
    assert_eq!(
        output.row(0).unwrap().revision,
        input.row(0).unwrap().revision
    );
    assert_eq!(
        output.row(0).unwrap().signature,
        input.row(0).unwrap().signature
    );
    assert!(previous.lookup(0, &input).is_none());
    assert!(previous.stale_lookup(0, &input).is_none());
    let (next, stats) = precompute_terminal_keyword_highlights_for_rows_with_stats(
        &input,
        &highlighter,
        palette,
        Some(&previous),
        0..1,
    );
    assert_eq!(stats.reused_rows, 0);
    assert_eq!(
        next.lookup(0, &input).unwrap().ranges().unwrap()[0].color,
        swatch("builtin-command")
    );
    let mut changed = input.clone();
    let mut rows = changed.rows().to_vec();
    Arc::make_mut(&mut rows[0]).shell_input_columns = Some((2, 30));
    changed.row_data = rows.into();
    assert!(next.lookup(0, &changed).is_none());
    assert!(next.stale_lookup(0, &changed).is_none());
}

#[test]
fn semantic_foreground_respects_ansi_color_and_hidden_text() {
    for ansi in ["\x1b[34m", "\x1b[38;2;12;34;56m", "\x1b[8m"] {
        let mut screen = TerminalScreen::new(20, 1);
        screen.advance(format!("{ansi}ERROR").as_bytes());
        let snapshot = screen.snapshot();
        let row = snapshot.row(0).unwrap();
        let spans = terminal_highlight_spans_with_keyword_ranges_and_options(
            &row.text,
            Some(&row.styled_spans),
            Some(&[TerminalKeywordRange {
                start_col: 0,
                end_col: 5,
                color: swatch("builtin-error"),
            }]),
            &[],
            zzclawterm_ui::theme_palette("github-dark"),
            false,
        );
        assert!(spans.iter().all(|span| !span.keyword));
        assert!(
            spans
                .iter()
                .all(|span| span.color != Some(swatch("builtin-error")))
        );
    }
}

#[test]
fn whole_match_ties_are_deterministic_and_user_marks_do_not_split_strings() {
    let mut rules = get_builtin_keyword_rules(true);
    let expected = SemanticHighlighter::compile(&rules);
    let line = "ERROR 192.168.1.1 128MiB 2026-10-07";
    let context = SemanticContext::default();
    let first = expected.ranges(line, &context, &[]);
    rules.reverse();
    assert_eq!(
        first,
        SemanticHighlighter::compile(&rules).ranges(line, &context, &[])
    );
    rules.insert(
        0,
        zzclawterm_core::keyword_highlight_presets::ResolvedKeywordHighlightRule {
            id: "user-mark".into(),
            name: "Mark".into(),
            patterns: vec!["ERROR".into()],
            color: "#123456".into(),
            enabled: true,
        },
    );
    let highlighter = compile_terminal_keyword_highlighter(&rules);
    let mut screen = TerminalScreen::new(40, 1);
    screen.advance(b"\x1b]133;B\x07echo \"ERROR\"");
    let snapshot = screen.snapshot();
    let highlights = precompute_terminal_keyword_highlights(
        &snapshot,
        &highlighter,
        zzclawterm_ui::theme_palette("github-dark"),
        None,
    );
    let ranges = highlights
        .lookup(0, &snapshot)
        .unwrap()
        .ranges()
        .unwrap()
        .to_vec();
    assert!(
        ranges
            .iter()
            .any(|r| r.start_col == 5 && r.end_col == 12 && r.color == swatch("builtin-string"))
    );
    assert!(ranges.iter().all(|r| r.color != 0x123456));

    // An exact whole-token tie still honors a user rule.
    let mut screen = TerminalScreen::new(40, 1);
    screen.advance(b"ERROR ERROR");
    let snapshot = screen.snapshot();
    let highlights = precompute_terminal_keyword_highlights(
        &snapshot,
        &highlighter,
        zzclawterm_ui::theme_palette("github-dark"),
        None,
    );
    assert!(
        highlights
            .lookup(0, &snapshot)
            .unwrap()
            .ranges()
            .unwrap()
            .iter()
            .all(|r| r.color == 0x123456)
    );
}

#[test]
fn category_switches_disable_structured_children_and_light_colors_remain_distinct() {
    let mut rules = get_builtin_keyword_rules(false);
    for rule in &mut rules {
        rule.enabled = !matches!(
            rule.id.as_str(),
            "builtin-permissions" | "builtin-string" | "builtin-command" | "builtin-path"
        );
    }
    let highlighter = SemanticHighlighter::compile(&rules);
    let line = "cat /tmp \"ERROR\n%s\"";
    let context = SemanticContext {
        input: std::iter::once(0..line.len()).collect(),
        integrated: true,
        ..SemanticContext::default()
    };
    let result = highlighter.matches(line, &context);
    assert!(result.iter().all(|m| m.priority != 10 && m.priority != 20));
    for id in [
        "builtin-command",
        "builtin-string",
        "builtin-path",
        "builtin-option",
        "builtin-number",
    ] {
        assert_ne!(
            builtin_keyword_rule_swatch(id, true),
            builtin_keyword_rule_swatch(id, false)
        );
    }
}

#[test]
#[ignore = "manual fixture-only background precompute benchmark"]
fn semantic_precompute_benchmark() {
    let highlighter = compile_terminal_keyword_highlighter(&get_builtin_keyword_rules(true));
    let palette = zzclawterm_ui::theme_palette("github-dark");
    for (case, fixture) in [
        ("plain", "ordinary output with calm filler"),
        (
            "dense",
            "drwxr-xr-x 18 root root 4096 Oct 7 21:37 ERROR 2001:db8::1 128MiB",
        ),
    ] {
        let mut screen = TerminalScreen::new(120, 200);
        screen.advance(
            std::iter::repeat_n(fixture, 200)
                .collect::<Vec<_>>()
                .join("\r\n")
                .as_bytes(),
        );
        let snapshot = screen.snapshot();
        let (first, stats) = precompute_terminal_keyword_highlights_for_rows_with_stats(
            &snapshot,
            &highlighter,
            palette,
            None,
            0..snapshot.row_count(),
        );
        let started = std::time::Instant::now();
        let (_, reused) = precompute_terminal_keyword_highlights_for_rows_with_stats(
            &snapshot,
            &highlighter,
            palette,
            Some(&first),
            0..snapshot.row_count(),
        );
        println!(
            "{case}: match_us={} map_us={} bytes={} ranges={} reuse_us={} reused_rows={}",
            stats.match_duration_us,
            stats.range_build_duration_us,
            stats.processed_bytes,
            stats.range_count,
            started.elapsed().as_micros(),
            reused.reused_rows
        );
        assert_eq!(reused.processed_bytes, 0);
        assert_eq!(reused.reused_rows, snapshot.row_count());
    }
}
