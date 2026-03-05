use serde_json::Value;

/// Section label for known stats keys.
fn section_label(key: &str) -> &str {
    match key {
        "free" => "Memory",
        "df" => "Disk",
        "uptime" => "Uptime",
        "ps" => "Processes",
        "ram" | "RAM" => "RAM",
        "dysk" | "disk" | "Disk" => "Disk",
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

/// Format KiB value to human-readable string.
fn format_kib(kib: u64) -> String {
    if kib >= 1_048_576 {
        format!("{:.1} GB", kib as f64 / 1_048_576.0)
    } else if kib >= 1024 {
        format!("{:.0} MB", kib as f64 / 1024.0)
    } else {
        format!("{} KB", kib)
    }
}

/// Truncate text to `max_width`, appending "..." if it was cut.
/// Returns the original string unchanged if it fits or `max_width` is 0.
fn truncate(text: &str, max_width: usize) -> String {
    if max_width == 0 || text.len() <= max_width {
        return text.to_string();
    }
    let cut = max_width.saturating_sub(3);
    format!("{}...", &text[..cut])
}

fn push_line(out: &mut String, line: &str, truncate_width: usize) {
    out.push_str(&truncate(line, truncate_width));
    out.push('\n');
}

/// Returns true for lines that are shell error noise (e.g. "sh: 1: echo", ": not found").
fn is_shell_noise(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("sh:") || t == ": not found"
}

/// Extract a JSON value as a plain string.
fn val_to_string(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

/// Format `free` command output: parse Mem/Swap lines and show progress bars.
fn format_free_section(out: &mut String, raw: &str, truncate_width: usize) {
    out.push_str("\nMemory\n");

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Mem:") || trimmed.starts_with("Swap:") {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
                let label = parts[0].trim_end_matches(':');
                if let (Ok(total), Ok(used)) = (
                    parts[1].parse::<u64>(),
                    parts[2].parse::<u64>(),
                ) {
                    let pct = if total > 0 {
                        (used as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    };
                    let bar = progress_bar(pct, 20);
                    push_line(
                        out,
                        &format!(
                            "  {:6} {}  {} / {}",
                            label,
                            bar,
                            format_kib(used),
                            format_kib(total)
                        ),
                        truncate_width,
                    );
                    continue;
                }
            }
        }
        // Skip header lines (total/used/free column headers)
        if trimmed.contains("total") && trimmed.contains("used") && trimmed.contains("free") {
            continue;
        }
        if !trimmed.is_empty() && !is_shell_noise(trimmed) {
            push_line(out, &format!("  {trimmed}"), truncate_width);
        }
    }
}

/// Format `df` command output: show progress bars for each filesystem line.
fn format_df_section(out: &mut String, raw: &str, truncate_width: usize) {
    out.push_str("\nDisk\n");

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("Filesystem") || is_shell_noise(trimmed) {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if let Some(pct) = extract_percentage(trimmed) {
            let bar = progress_bar(pct, 20);
            // Parse df fields: ..., Size, Used, Avail, Use%, Mounted_on
            // Use mount point as label to align with Memory section bars.
            if parts.len() >= 6 {
                let size = parts[parts.len() - 5];
                let used = parts[parts.len() - 4];
                let mount = parts[parts.len() - 1];
                push_line(
                    out,
                    &format!("  {:6} {}  {} / {}", mount, bar, used, size),
                    truncate_width,
                );
            } else {
                push_line(out, &format!("  {:6} {}  {}", "", bar, trimmed), truncate_width);
            }
        } else {
            push_line(out, &format!("  {trimmed}"), truncate_width);
        }
    }
}

/// Format `uptime` command output.
fn format_uptime_section(out: &mut String, raw: &str, truncate_width: usize) {
    out.push_str("\nUptime\n");

    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !is_shell_noise(trimmed) {
            push_line(out, &format!("  {trimmed}"), truncate_width);
        }
    }
}

/// Format `ps` command output.
fn format_ps_section(out: &mut String, raw: &str, truncate_width: usize) {
    out.push_str("\nProcesses\n");

    for line in raw.lines() {
        if !line.trim().is_empty() && !is_shell_noise(line) {
            push_line(out, &format!("  {}", line.trim()), truncate_width);
        }
    }
}

/// Generic section for unknown keys — show a bar if a percentage is found.
fn format_generic_section(out: &mut String, key: &str, raw: &str, truncate_width: usize) {
    let label = section_label(key);
    out.push_str(&format!("\n{label}\n"));

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let formatted = match extract_percentage(trimmed) {
            Some(pct) => format!("  {}  {}", progress_bar(pct, 20), trimmed),
            None => format!("  {trimmed}"),
        };
        push_line(out, &formatted, truncate_width);
    }
}

/// Convert a snake_case key to a human-readable label: `server_id` → `Server Id`.
fn humanize_key(key: &str) -> String {
    if key.is_empty() {
        return String::new();
    }
    key.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    format!("{upper}{}", chars.as_str())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render a JSON leaf value as a plain string.
fn format_scalar(val: &Value) -> String {
    match val {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => if *b { "Yes" } else { "No" }.to_string(),
        Value::Null => "-".to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// Render an object as aligned key-value pairs.
fn format_object(map: &serde_json::Map<String, Value>, indent: usize) -> String {
    let prefix = " ".repeat(indent);
    let labels: Vec<String> = map.keys().map(|k| humanize_key(k)).collect();
    let max_label = labels.iter().map(|l| l.len()).max().unwrap_or(0);

    let mut out = String::new();
    for (label, (_key, val)) in labels.iter().zip(map.iter()) {
        match val {
            Value::Object(inner) => {
                out.push_str(&format!("{prefix}{label}:\n"));
                out.push_str(&format_object(inner, indent + 2));
            }
            Value::Array(arr) => {
                out.push_str(&format!("{prefix}{label}:\n"));
                out.push_str(&format_array(arr, indent + 2));
            }
            _ => {
                let value_str = format_scalar(val);
                out.push_str(&format!(
                    "{prefix}{:<width$}  {}\n",
                    format!("{label}:"),
                    value_str,
                    width = max_label + 1
                ));
            }
        }
    }
    out
}

/// Render an array as numbered entries (objects) or bulleted items (scalars).
fn format_array(arr: &[Value], indent: usize) -> String {
    let prefix = " ".repeat(indent);
    if arr.is_empty() {
        return format!("{prefix}(empty)\n");
    }

    let mut out = String::new();
    let all_objects = arr.iter().all(|v| v.is_object());

    if all_objects {
        for (i, item) in arr.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let num_prefix = format!("{prefix}{}. ", i + 1);
            if let Value::Object(map) = item {
                let inner = format_object(map, 0);
                for (j, line) in inner.lines().enumerate() {
                    if j == 0 {
                        out.push_str(&format!("{num_prefix}{line}\n"));
                    } else {
                        out.push_str(&format!(
                            "{}{line}\n",
                            " ".repeat(num_prefix.len())
                        ));
                    }
                }
            }
        }
    } else {
        for item in arr {
            out.push_str(&format!("{prefix}{}\n", format_scalar(item)));
        }
    }
    out
}

/// Format a JSON API response as human-readable text.
///
/// Special cases:
/// - `exec` command: if the response has an `"output"` key, print just that value.
/// - Otherwise delegates to the appropriate renderer based on JSON type.
pub fn format_value(value: &Value, command: &str) -> String {
    // Special case: exec command with "output" key
    if command == "exec" {
        if let Value::Object(map) = value {
            if let Some(output) = map.get("output") {
                let text = format_scalar(output);
                // Trim trailing whitespace but ensure trailing newline
                let trimmed = text.trim_end();
                if trimmed.is_empty() {
                    return String::new();
                }
                return format!("{trimmed}\n");
            }
        }
    }

    match value {
        Value::Object(map) => format_object(map, 0),
        Value::Array(arr) => format_array(arr, 0),
        _ => {
            let s = format_scalar(value);
            if s.is_empty() {
                s
            } else {
                format!("{s}\n")
            }
        }
    }
}

/// Format the stats API response as a human-readable string.
/// If `truncate_width` is non-zero, long lines are cut and end with "...".
pub fn format_stats(value: &Value, truncate_width: usize) -> String {
    let mut out = String::new();

    match value {
        Value::Object(map) => {
            // Render known sections in a logical order first.
            let ordered_keys = ["free", "df", "uptime", "ps"];

            for &key in &ordered_keys {
                if let Some(val) = map.get(key) {
                    let raw = val_to_string(val);
                    match key {
                        "free" => format_free_section(&mut out, &raw, truncate_width),
                        "df" => format_df_section(&mut out, &raw, truncate_width),
                        "uptime" => format_uptime_section(&mut out, &raw, truncate_width),
                        "ps" => format_ps_section(&mut out, &raw, truncate_width),
                        _ => unreachable!(),
                    }
                }
            }

            // Render any remaining keys not in the ordered list.
            for (key, val) in map {
                if ordered_keys.contains(&key.as_str()) {
                    continue;
                }
                let raw = val_to_string(val);
                format_generic_section(&mut out, key, &raw, truncate_width);
            }
        }
        other => {
            out.push_str(&serde_json::to_string_pretty(other).unwrap_or_default());
            out.push('\n');
        }
    }

    // Remove leading newline from first section.
    if out.starts_with('\n') {
        out.remove(0);
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
    fn test_format_kib() {
        assert_eq!(format_kib(0), "0 KB");
        assert_eq!(format_kib(512), "512 KB");
        assert_eq!(format_kib(1024), "1 MB");
        assert_eq!(format_kib(262144), "256 MB");
        assert_eq!(format_kib(1_048_576), "1.0 GB");
        assert_eq!(format_kib(5_242_880), "5.0 GB");
    }

    #[test]
    fn test_format_free_section() {
        let free_output = "\
              total        used        free      shared  buff/cache   available
Mem:         262144      163840       32768       16384       65536       98304
Swap:        524288       52428      471860";

        let mut out = String::new();
        format_free_section(&mut out, free_output, 0);
        assert!(out.contains("Memory"));
        assert!(out.contains("Mem"));
        assert!(out.contains("Swap"));
        // Mem: 163840/262144 = 62.5%
        assert!(out.contains("62.5%"));
        // Swap: 52428/524288 = 10.0%
        assert!(out.contains("10.0%"));
        // Should have progress bars
        assert!(out.contains('['));
        // Should show human-readable sizes
        assert!(out.contains("160 MB"));
        assert!(out.contains("256 MB"));
    }

    #[test]
    fn test_format_free_zero_swap() {
        let free_output = "\
              total        used        free      shared  buff/cache   available
Mem:         262144      131072       65536       16384       65536      131072
Swap:              0           0           0";

        let mut out = String::new();
        format_free_section(&mut out, free_output, 0);
        assert!(out.contains("Mem"));
        assert!(out.contains("50.0%"));
        // Swap with total=0 should still show a bar at 0%
        assert!(out.contains("Swap"));
        assert!(out.contains("0.0%"));
        assert!(out.contains("0 KB / 0 KB"));
    }

    #[test]
    fn test_format_df_section() {
        let df_output = "\
Filesystem     1K-blocks    Used Available Use% Mounted on
/dev/vda1       5242880 1258292   3984588  24% /";

        let mut out = String::new();
        format_df_section(&mut out, df_output, 0);
        assert!(out.contains("Disk"));
        assert!(out.contains("24.0%"));
        assert!(out.contains('['));
        // Header line should be skipped
        assert!(!out.contains("Filesystem"));
        // Should show parsed used/size/mount with mount as label
        assert!(out.contains("1258292 / 5242880"));
    }

    #[test]
    fn test_format_df_human_readable() {
        let df_output = "\
Filesystem                             Size  Used Avail Use% Mounted on
/dev/mapper/pve-vm--245--disk--0        44G  6.5G   36G  16% /";

        let mut out = String::new();
        format_df_section(&mut out, df_output, 0);
        assert!(out.contains("16.0%"));
        assert!(out.contains("6.5G / 44G"));
    }

    #[test]
    fn test_format_uptime_section() {
        let uptime_output =
            " 10:23:45 up 10 days, 5:23, 0 users, load average: 0.00, 0.01, 0.05";

        let mut out = String::new();
        format_uptime_section(&mut out, uptime_output, 0);
        assert!(out.contains("Uptime"));
        assert!(out.contains("10 days"));
        assert!(out.contains("load average"));
    }

    #[test]
    fn test_format_uptime_filters_shell_noise() {
        let uptime_output =
            " 10:23:45 up 10 days, 5:23, 0 users, load average: 0.00, 0.01, 0.05\nsh: 1: echo";

        let mut out = String::new();
        format_uptime_section(&mut out, uptime_output, 0);
        assert!(out.contains("10 days"));
        assert!(!out.contains("sh:"));
    }

    #[test]
    fn test_format_ps_section() {
        let ps_output = "\
USER       PID %CPU %MEM    VSZ   RSS TTY      STAT START   TIME COMMAND
root         1  0.0  0.5  19356  1404 ?        Ss   Jan01   0:05 /sbin/init";

        let mut out = String::new();
        format_ps_section(&mut out, ps_output, 0);
        assert!(out.contains("Processes"));
        assert!(out.contains("root"));
        assert!(out.contains("/sbin/init"));
    }

    #[test]
    fn test_format_ps_filters_shell_noise() {
        let ps_output = "\
: not found
USER       PID %CPU %MEM    VSZ   RSS TTY      STAT START   TIME COMMAND
root         1  0.0  0.5  19356  1404 ?        Ss   Jan01   0:05 /sbin/init";

        let mut out = String::new();
        format_ps_section(&mut out, ps_output, 0);
        assert!(!out.contains(": not found"));
        assert!(out.contains("root"));
    }

    #[test]
    fn test_format_stats_real_api_response() {
        let stats = json!({
            "free": "              total        used        free      shared  buff/cache   available\nMem:         262144      163840       32768       16384       65536       98304\nSwap:        524288       52428      471860",
            "df": "Filesystem                             Size  Used Avail Use% Mounted on\n/dev/mapper/pve-vm--245--disk--0        44G  6.5G   36G  16% /",
            "uptime": " 10:23:45 up 10 days, 5:23, 0 users, load average: 0.00, 0.01, 0.05\nsh: 1: echo",
            "ps": ": not found\nUSER       PID %CPU %MEM    VSZ   RSS TTY      STAT START   TIME COMMAND\nroot         1  0.0  0.5  19356  1404 ?        Ss   Jan01   0:05 /sbin/init"
        });
        let output = format_stats(&stats, 0);
        // No title header
        assert!(!output.contains("Server Statistics"));
        assert!(!output.contains("\u{2500}"));
        // Memory section should have bars for both Mem and Swap
        assert!(output.contains("Memory"));
        assert!(output.contains("62.5%"));
        assert!(output.contains("10.0%"));
        // Disk section should have bars
        assert!(output.contains("Disk"));
        assert!(output.contains("16.0%"));
        assert!(output.contains("6.5G / 44G"));
        // Uptime section, shell noise filtered
        assert!(output.contains("Uptime"));
        assert!(output.contains("10 days"));
        assert!(!output.contains("sh:"));
        // Processes section, shell noise filtered
        assert!(output.contains("Processes"));
        assert!(output.contains("/sbin/init"));
        assert!(!output.contains(": not found"));
        // Starts with Memory (first section, no leading blank line)
        assert!(output.starts_with("Memory\n"));
    }

    #[test]
    fn test_format_stats_generic_fallback() {
        let stats = json!({
            "ram": "128/256MB (50%)",
            "dysk": "1.2/5GB (24%)",
            "uptime": "10 days, 5:23"
        });
        let output = format_stats(&stats, 0);
        // uptime key matches ordered list but uses generic since value is plain text
        assert!(output.contains("Uptime"));
        assert!(output.contains("10 days, 5:23"));
        // ram and dysk use generic formatter with progress bars
        assert!(output.contains("RAM"));
        assert!(output.contains("Disk"));
        assert!(output.contains('['));
    }

    #[test]
    fn test_format_stats_non_object() {
        let stats = json!("just a string");
        let output = format_stats(&stats, 80);
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
            "custom": "this is a very long value that should definitely be truncated when the truncate width is set to a small number"
        });
        let output = format_stats(&stats, 40);
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            assert!(
                line.len() <= 40,
                "line too long: {:?} ({})",
                line,
                line.len()
            );
        }
    }

    #[test]
    fn test_section_label_mapping() {
        assert_eq!(section_label("free"), "Memory");
        assert_eq!(section_label("df"), "Disk");
        assert_eq!(section_label("uptime"), "Uptime");
        assert_eq!(section_label("ps"), "Processes");
        assert_eq!(section_label("ram"), "RAM");
        assert_eq!(section_label("dysk"), "Disk");
        assert_eq!(section_label("unknown_field"), "unknown_field");
    }

    #[test]
    fn test_sections_appear_in_order() {
        let stats = json!({
            "ps": "USER PID\nroot 1",
            "df": "Filesystem Use%\n/dev/vda1 24%",
            "uptime": "up 10 days",
            "free": "              total  used  free\nMem:  262144  131072  131072\nSwap: 524288  0  524288"
        });
        let output = format_stats(&stats, 0);
        let mem_pos = output.find("Memory").unwrap();
        let disk_pos = output.find("Disk").unwrap();
        let uptime_pos = output.find("Uptime").unwrap();
        let proc_pos = output.find("Processes").unwrap();
        assert!(mem_pos < disk_pos, "Memory should come before Disk");
        assert!(disk_pos < uptime_pos, "Disk should come before Uptime");
        assert!(uptime_pos < proc_pos, "Uptime should come before Processes");
    }

    #[test]
    fn test_all_content_lines_are_indented() {
        let stats = json!({
            "free": "              total  used  free\nMem:  262144  131072  131072\nSwap: 524288  0  524288",
            "df": "Filesystem Size Used Avail Use% Mounted\n/dev/vda1 44G 6.5G 36G 16% /",
            "uptime": "up 10 days, load average: 0.01, 0.02, 0.03",
            "ps": "USER PID\nroot 1 /sbin/init"
        });
        let output = format_stats(&stats, 0);
        for line in output.lines() {
            if line.trim().is_empty() {
                continue;
            }
            // Section headers have no indent
            if ["Memory", "Disk", "Uptime", "Processes"].contains(&line.trim()) {
                assert!(!line.starts_with(' '), "headers should not be indented: {line:?}");
            } else {
                // Content lines should start with "  "
                assert!(
                    line.starts_with("  "),
                    "content line should be indented: {line:?}"
                );
            }
        }
    }

    #[test]
    fn test_is_shell_noise() {
        assert!(is_shell_noise("sh: 1: echo"));
        assert!(is_shell_noise(": not found"));
        assert!(is_shell_noise("  sh: 1: echo  "));
        assert!(!is_shell_noise("root 1 /sbin/init"));
        assert!(!is_shell_noise("USER PID"));
    }

    // --- Tests for generic formatting functions ---

    #[test]
    fn test_humanize_key_snake_case() {
        assert_eq!(humanize_key("server_id"), "Server Id");
    }

    #[test]
    fn test_humanize_key_single_word() {
        assert_eq!(humanize_key("name"), "Name");
    }

    #[test]
    fn test_humanize_key_empty() {
        assert_eq!(humanize_key(""), "");
    }

    #[test]
    fn test_humanize_key_multi_underscore() {
        assert_eq!(humanize_key("last_login_date"), "Last Login Date");
    }

    #[test]
    fn test_format_scalar_string() {
        assert_eq!(format_scalar(&json!("hello")), "hello");
    }

    #[test]
    fn test_format_scalar_number() {
        assert_eq!(format_scalar(&json!(42)), "42");
    }

    #[test]
    fn test_format_scalar_bool() {
        assert_eq!(format_scalar(&json!(true)), "Yes");
        assert_eq!(format_scalar(&json!(false)), "No");
    }

    #[test]
    fn test_format_scalar_null() {
        assert_eq!(format_scalar(&json!(null)), "-");
    }

    #[test]
    fn test_format_value_flat_object() {
        let val = json!({"server_id": "12345", "ram": "256MB"});
        let output = format_value(&val, "info");
        assert!(output.contains("Server Id:"));
        assert!(output.contains("12345"));
        assert!(output.contains("Ram:"));
        assert!(output.contains("256MB"));
    }

    #[test]
    fn test_format_value_key_alignment() {
        let val = json!({"id": "1", "server_name": "srv1"});
        let output = format_value(&val, "info");
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        // Values should start at the same column (padding aligns them)
        let value_positions: Vec<usize> = lines
            .iter()
            .map(|l| {
                let colon = l.find(':').unwrap();
                // After "Label:" there are spaces then the value
                let after_colon = &l[colon + 1..];
                let trimmed = after_colon.trim_start();
                l.len() - after_colon.len() + (after_colon.len() - trimmed.len())
            })
            .collect();
        assert_eq!(
            value_positions[0], value_positions[1],
            "values should be aligned: {:?}",
            lines
        );
    }

    #[test]
    fn test_format_value_array_of_objects() {
        let val = json!([
            {"id": 1, "name": "srv1"},
            {"id": 2, "name": "srv2"}
        ]);
        let output = format_value(&val, "servers");
        assert!(output.contains("1."));
        assert!(output.contains("2."));
        assert!(output.contains("srv1"));
        assert!(output.contains("srv2"));
    }

    #[test]
    fn test_format_value_exec_special_case() {
        let val = json!({"output": "up 10 days\n"});
        let output = format_value(&val, "exec");
        assert_eq!(output, "up 10 days\n");
    }

    #[test]
    fn test_format_value_exec_no_output_key() {
        let val = json!({"error": "command not found"});
        let output = format_value(&val, "exec");
        // Falls through to generic object formatting
        assert!(output.contains("Error:"));
        assert!(output.contains("command not found"));
    }

    #[test]
    fn test_format_value_nested_object() {
        let val = json!({"server": {"id": 1, "name": "srv1"}});
        let output = format_value(&val, "info");
        assert!(output.contains("Server:"));
        assert!(output.contains("Id:"));
        assert!(output.contains("srv1"));
    }

    #[test]
    fn test_format_value_empty_array() {
        let val = json!([]);
        let output = format_value(&val, "servers");
        assert!(output.contains("(empty)"));
    }

    #[test]
    fn test_format_value_scalar_array() {
        let val = json!(["one", "two", "three"]);
        let output = format_value(&val, "test");
        assert!(output.contains("one"));
        assert!(output.contains("two"));
        assert!(output.contains("three"));
        assert!(!output.contains("- "));
    }

    #[test]
    fn test_format_value_plain_string() {
        let val = json!("just a message");
        let output = format_value(&val, "test");
        assert_eq!(output, "just a message\n");
    }
}
