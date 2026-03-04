use serde_json::Value;

/// Known field labels for the stats response (Polish API keys -> English labels).
fn field_label(key: &str) -> &str {
    match key {
        "ram" | "RAM" => "RAM",
        "dysk" | "disk" | "Disk" => "Disk",
        "uptime" | "Uptime" => "Uptime",
        "hdd" | "HDD" => "HDD",
        "cpu" | "CPU" => "CPU",
        "swap" | "Swap" => "Swap",
        _ => key,
    }
}

/// Try to extract a percentage (0-100) from a string like "50%", "128/256MB (50%)", etc.
fn extract_percentage(s: &str) -> Option<f64> {
    // Look for pattern like (XX%) or (XX.X%)
    if let Some(start) = s.rfind('(') {
        if let Some(end) = s[start..].find("%)") {
            let num_str = &s[start + 1..start + end];
            if let Ok(pct) = num_str.trim().parse::<f64>() {
                return Some(pct);
            }
        }
    }
    // Look for trailing XX% pattern
    for word in s.split_whitespace().rev() {
        if let Some(num_str) = word.strip_suffix('%') {
            if let Ok(pct) = num_str.parse::<f64>() {
                return Some(pct);
            }
        }
    }
    None
}

/// Render a progress bar: [████████░░░░░░░░] XX%
fn progress_bar(percentage: f64, width: usize) -> String {
    let pct = percentage.clamp(0.0, 100.0);
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    let bar_char = if pct >= 90.0 {
        '\u{2593}' // ▓ dark shade for critical
    } else if pct >= 70.0 {
        '\u{2592}' // ▒ medium shade for warning
    } else {
        '\u{2588}' // █ full block for normal
    };

    format!(
        "[{}{}] {:5.1}%",
        std::iter::repeat(bar_char).take(filled).collect::<String>(),
        std::iter::repeat('\u{2591}')
            .take(empty)
            .collect::<String>(),
        pct
    )
}

/// Format a single stats value — with a bar if a percentage is found.
fn format_value(value: &str) -> String {
    match extract_percentage(value) {
        Some(pct) => format!("{}  {}", progress_bar(pct, 20), value),
        None => value.to_string(),
    }
}

/// Truncate text to `max_width`, appending "..." if it was cut.
/// Returns the original string unchanged if it fits or `max_width` is 0.
fn truncate(text: &str, max_width: usize) -> String {
    if max_width == 0 || text.len() <= max_width {
        return text.to_string();
    }
    // Need at least 3 chars for the ellipsis to make sense.
    let cut = max_width.saturating_sub(3);
    format!("{}...", &text[..cut])
}

/// Format the stats API response as a human-readable string.
/// If `truncate_width` is non-zero, long lines are cut and end with "...".
pub fn format_stats(value: &Value, truncate_width: usize) -> String {
    let mut out = String::new();

    out.push_str("Server Statistics\n");
    out.push_str(&"\u{2500}".repeat(40));
    out.push('\n');

    match value {
        Value::Object(map) => {
            // Compute the max label width for alignment.
            let max_label_len = map
                .keys()
                .map(|k| field_label(k).len())
                .max()
                .unwrap_or(0);

            // Prefix: "  " + label + "  "
            let prefix_width = 2 + max_label_len + 2;
            let continuation_prefix: String = " ".repeat(prefix_width);

            for (key, val) in map {
                let label = field_label(key);
                let raw = match val {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    other => serde_json::to_string_pretty(other).unwrap_or_default(),
                };

                let formatted = format_value(&raw);
                for (i, line) in formatted.lines().enumerate() {
                    let full_line = if i == 0 {
                        format!("  {:width$}  {}", label, line, width = max_label_len)
                    } else {
                        format!("{}{}", continuation_prefix, line)
                    };
                    out.push_str(&truncate(&full_line, truncate_width));
                    out.push('\n');
                }
            }
        }
        // If the response is not an object, just print it nicely.
        other => {
            out.push_str(&serde_json::to_string_pretty(other).unwrap_or_default());
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_percentage_parenthesized() {
        assert_eq!(extract_percentage("128/256MB (50%)"), Some(50.0));
        assert_eq!(extract_percentage("1.2/5GB (24.5%)"), Some(24.5));
    }

    #[test]
    fn test_extract_percentage_trailing() {
        assert_eq!(extract_percentage("50%"), Some(50.0));
        assert_eq!(extract_percentage("used 75%"), Some(75.0));
    }

    #[test]
    fn test_extract_percentage_none() {
        assert_eq!(extract_percentage("10 days, 5:23"), None);
        assert_eq!(extract_percentage("no percentage here"), None);
    }

    #[test]
    fn test_progress_bar_zero() {
        let bar = progress_bar(0.0, 20);
        assert!(bar.contains("0.0%"));
        assert!(bar.starts_with('['));
        assert!(bar.contains(']'));
    }

    #[test]
    fn test_progress_bar_full() {
        let bar = progress_bar(100.0, 20);
        assert!(bar.contains("100.0%"));
    }

    #[test]
    fn test_progress_bar_warning_shade() {
        let bar = progress_bar(75.0, 20);
        assert!(bar.contains('\u{2592}')); // medium shade
    }

    #[test]
    fn test_progress_bar_critical_shade() {
        let bar = progress_bar(95.0, 20);
        assert!(bar.contains('\u{2593}')); // dark shade
    }

    #[test]
    fn test_format_stats_object() {
        let stats = json!({
            "ram": "128/256MB (50%)",
            "dysk": "1.2/5GB (24%)",
            "uptime": "10 days, 5:23"
        });
        let output = format_stats(&stats, 80);
        assert!(output.contains("Server Statistics"));
        assert!(output.contains("RAM"));
        assert!(output.contains("Disk"));
        assert!(output.contains("Uptime"));
        assert!(output.contains("10 days, 5:23"));
        // RAM and Disk should have progress bars
        assert!(output.contains('['));
    }

    #[test]
    fn test_format_stats_non_object() {
        let stats = json!("just a string");
        let output = format_stats(&stats, 80);
        assert!(output.contains("Server Statistics"));
        assert!(output.contains("just a string"));
    }

    #[test]
    fn test_truncate_no_cut_needed() {
        assert_eq!(truncate("short text", 80), "short text");
    }

    #[test]
    fn test_truncate_disabled() {
        let long = "a ".repeat(50);
        assert_eq!(truncate(&long, 0), long);
    }

    #[test]
    fn test_truncate_cuts_and_adds_ellipsis() {
        let text = "this is a long string that should be cut";
        let result = truncate(text, 20);
        assert_eq!(result.len(), 20);
        assert!(result.ends_with("..."));
        assert_eq!(result, "this is a long st...");
    }

    #[test]
    fn test_format_stats_truncation() {
        let stats = json!({
            "info": "this is a very long value that should definitely be truncated when the truncate width is set to a small number"
        });
        let output = format_stats(&stats, 40);
        for line in output.lines().skip(2) {
            // content lines should be at most 40 chars
            assert!(line.len() <= 40, "line too long: {:?} ({})", line, line.len());
        }
    }

    #[test]
    fn test_field_label_mapping() {
        assert_eq!(field_label("ram"), "RAM");
        assert_eq!(field_label("dysk"), "Disk");
        assert_eq!(field_label("uptime"), "Uptime");
        assert_eq!(field_label("unknown_field"), "unknown_field");
    }
}
