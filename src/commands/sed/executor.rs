// Executor for sed commands

use std::collections::HashMap;
use regex_lite::Regex;
use super::types::{SedAddress, AddressRange, SedCmd, SedState, RangeState, ExecuteContext,
                   PendingFileRead, PendingFileWrite};
use super::regex_utils::{bre_to_ere, normalize_for_rust, escape_for_list};

const DEFAULT_MAX_ITERATIONS: usize = 10_000;

/// Create a fresh SedState with given total_lines, filename, and range_states.
pub fn create_initial_state(
    total_lines: usize,
    filename: Option<&str>,
    range_states: HashMap<String, RangeState>,
) -> SedState {
    let mut state = SedState::new(total_lines);
    state.current_filename = filename.map(|s| s.to_string());
    state.range_states = range_states;
    state
}

/// Serialize an address range to a string for use as a HashMap key.
fn serialize_range(range: &AddressRange) -> String {
    fn serialize_addr(addr: &Option<SedAddress>) -> String {
        match addr {
            None => "undefined".to_string(),
            Some(SedAddress::Last) => "$".to_string(),
            Some(SedAddress::Line(n)) => n.to_string(),
            Some(SedAddress::Pattern(p)) => format!("/{}/", p),
            Some(SedAddress::Step { first, step }) => format!("{}~{}", first, step),
            Some(SedAddress::RelativeOffset(n)) => format!("+{}", n),
        }
    }
    format!("{},{}", serialize_addr(&range.start), serialize_addr(&range.end))
}

/// Check if a single address matches the current line.
fn matches_address(
    addr: &SedAddress,
    line_num: usize,
    total_lines: usize,
    line: &str,
    state: &mut SedState,
) -> bool {
    match addr {
        SedAddress::Line(n) => line_num == *n,
        SedAddress::Last => line_num == total_lines,
        SedAddress::Step { first, step } => {
            if *step == 0 {
                return line_num == *first;
            }
            line_num >= *first && (line_num - first) % step == 0
        }
        SedAddress::Pattern(p) => {
            // Handle empty pattern (reuse last pattern)
            let raw_pattern = if p.is_empty() {
                if let Some(ref last) = state.last_pattern {
                    last.clone()
                } else {
                    return false;
                }
            } else {
                state.last_pattern = Some(p.clone());
                p.clone()
            };
            let pattern = normalize_for_rust(&bre_to_ere(&raw_pattern));
            match Regex::new(&pattern) {
                Ok(re) => re.is_match(line),
                Err(_) => false,
            }
        }
        SedAddress::RelativeOffset(_) => false, // Handled in is_in_range
    }
}

/// Internal range check without negation handling.
fn is_in_range_internal(
    range: &Option<AddressRange>,
    line_num: usize,
    total_lines: usize,
    line: &str,
    state: &mut SedState,
) -> bool {
    let range = match range {
        None => return true,
        Some(r) => r,
    };

    if range.start.is_none() && range.end.is_none() {
        return true;
    }

    let start = &range.start;
    let end = &range.end;

    if start.is_some() && end.is_none() {
        // Single address
        return matches_address(start.as_ref().unwrap(), line_num, total_lines, line, state);
    }

    if let (Some(start_addr), Some(end_addr)) = (start, end) {
        let has_pattern_start = matches!(start_addr, SedAddress::Pattern(_));
        let has_pattern_end = matches!(end_addr, SedAddress::Pattern(_));
        let has_relative_end = matches!(end_addr, SedAddress::RelativeOffset(_));

        // Handle relative offset end address (GNU extension: /pattern/,+N)
        if has_relative_end {
            let offset = match end_addr {
                SedAddress::RelativeOffset(n) => *n,
                _ => unreachable!(),
            };
            let range_key = serialize_range(range);
            let range_state = state.range_states.entry(range_key.clone())
                .or_insert_with(RangeState::default);

            if !range_state.active {
                let start_matches = matches_address(start_addr, line_num, total_lines, line, state);
                if start_matches {
                    let rs = state.range_states.get_mut(&range_key).unwrap();
                    rs.active = true;
                    rs.start_line = Some(line_num);
                    if offset == 0 {
                        rs.active = false;
                    }
                    return true;
                }
                return false;
            } else {
                let start_line = range_state.start_line.unwrap_or(line_num);
                if line_num >= start_line + offset {
                    let rs = state.range_states.get_mut(&range_key).unwrap();
                    rs.active = false;
                }
                return true;
            }
        }

        // If both are non-pattern, non-relative (numeric/last)
        if !has_pattern_start && !has_pattern_end && !has_relative_end {
            let start_num = match start_addr {
                SedAddress::Line(n) => *n,
                SedAddress::Last => total_lines,
                SedAddress::Step { first, .. } => *first,
                _ => 1,
            };
            let end_num = match end_addr {
                SedAddress::Line(n) => *n,
                SedAddress::Last => total_lines,
                SedAddress::Step { first, .. } => *first,
                _ => total_lines,
            };

            if start_num <= end_num {
                return line_num >= start_num && line_num <= end_num;
            }

            // Backward range - use state tracking
            let range_key = serialize_range(range);
            let range_state = state.range_states.entry(range_key.clone())
                .or_insert_with(RangeState::default);

            if !range_state.completed {
                if line_num >= start_num {
                    let rs = state.range_states.get_mut(&range_key).unwrap();
                    rs.completed = true;
                    return true;
                }
            }
            return false;
        }

        // For pattern ranges, use state tracking
        let range_key = serialize_range(range);
        let range_state = state.range_states.entry(range_key.clone())
            .or_insert_with(RangeState::default);

        if !range_state.active {
            if range_state.completed {
                return false;
            }

            let start_matches = match start_addr {
                SedAddress::Line(n) => line_num >= *n,
                _ => matches_address(start_addr, line_num, total_lines, line, state),
            };

            if start_matches {
                {
                    let rs = state.range_states.get_mut(&range_key).unwrap();
                    rs.active = true;
                    rs.start_line = Some(line_num);
                }

                // Check if end also matches on the same line
                if matches_address(end_addr, line_num, total_lines, line, state) {
                    let rs = state.range_states.get_mut(&range_key).unwrap();
                    rs.active = false;
                    if matches!(start_addr, SedAddress::Line(_)) {
                        rs.completed = true;
                    }
                }
                return true;
            }
            return false;
        } else {
            // Already in range - check if end matches
            if matches_address(end_addr, line_num, total_lines, line, state) {
                let rs = state.range_states.get_mut(&range_key).unwrap();
                rs.active = false;
                if matches!(start_addr, SedAddress::Line(_)) {
                    rs.completed = true;
                }
            }
            return true;
        }
    }

    true
}

/// Check if the current line is in the address range, handling negation.
fn is_in_range(
    range: &Option<AddressRange>,
    line_num: usize,
    total_lines: usize,
    line: &str,
    state: &mut SedState,
) -> bool {
    let result = is_in_range_internal(range, line_num, total_lines, line, state);

    if let Some(ref r) = range {
        if r.negated {
            return !result;
        }
    }

    result
}

/// Custom global replacement function that handles zero-length matches correctly.
/// POSIX sed behavior:
/// 1. After a zero-length match: replace, then advance by 1 char, output that char
/// 2. After a non-zero-length match: if next position would be a zero-length match, skip it
fn global_replace(
    input: &str,
    regex: &Regex,
    replace_fn: impl Fn(&str, &[&str]) -> String,
) -> String {
    let mut result = String::new();
    let mut pos = 0;
    let bytes = input.as_bytes();
    let mut skip_zero_length_at_next_pos = false;

    while pos <= input.len() {
        // Search in the substring starting at pos
        let haystack = &input[pos..];
        let caps = regex.captures(haystack);

        match caps {
            None => {
                // No match found - output remaining characters
                result.push_str(haystack);
                break;
            }
            Some(caps) => {
                let m = caps.get(0).unwrap();

                if m.start() != 0 {
                    // Match found but not at start of substring - output chars up to match
                    result.push_str(&haystack[..m.start()]);
                    pos += m.start();
                    skip_zero_length_at_next_pos = false;
                    continue;
                }

                // Match found at current position
                let matched_text = m.as_str();

                // After a non-zero match, skip zero-length matches at the boundary
                if skip_zero_length_at_next_pos && matched_text.is_empty() {
                    if pos < input.len() {
                        // Output one character and advance
                        let ch_len = char_len_at(bytes, pos);
                        result.push_str(&input[pos..pos + ch_len]);
                        pos += ch_len;
                    } else {
                        break;
                    }
                    skip_zero_length_at_next_pos = false;
                    continue;
                }

                // Get capture groups
                let groups: Vec<&str> = (1..caps.len())
                    .map(|i| caps.get(i).map_or("", |m| m.as_str()))
                    .collect();

                // Apply replacement
                result.push_str(&replace_fn(matched_text, &groups));
                skip_zero_length_at_next_pos = false;

                if matched_text.is_empty() {
                    // Zero-length match: advance by 1 char, output that char
                    if pos < input.len() {
                        let ch_len = char_len_at(bytes, pos);
                        result.push_str(&input[pos..pos + ch_len]);
                        pos += ch_len;
                    } else {
                        break;
                    }
                } else {
                    // Non-zero-length match: advance by match length
                    pos += matched_text.len();
                    skip_zero_length_at_next_pos = true;
                }
            }
        }
    }

    result
}

/// Get the byte length of the UTF-8 character at the given byte position.
fn char_len_at(bytes: &[u8], pos: usize) -> usize {
    if pos >= bytes.len() {
        return 1;
    }
    let b = bytes[pos];
    if b < 0x80 { 1 }
    else if b < 0xE0 { 2 }
    else if b < 0xF0 { 3 }
    else { 4 }
}

/// Process replacement string, expanding &, \0-\9, \n, \t, \r, \\, \&.
pub fn process_replacement(replacement: &str, match_text: &str, groups: &[&str]) -> String {
    let chars: Vec<char> = replacement.chars().collect();
    let mut result = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' {
            if i + 1 < chars.len() {
                let next = chars[i + 1];
                match next {
                    '&' => {
                        result.push('&');
                        i += 2;
                        continue;
                    }
                    'n' => {
                        result.push('\n');
                        i += 2;
                        continue;
                    }
                    't' => {
                        result.push('\t');
                        i += 2;
                        continue;
                    }
                    'r' => {
                        result.push('\r');
                        i += 2;
                        continue;
                    }
                    '0' => {
                        // \0 is the entire match (same as &)
                        result.push_str(match_text);
                        i += 2;
                        continue;
                    }
                    '1'..='9' => {
                        let digit = (next as u8 - b'0') as usize;
                        if digit <= groups.len() {
                            result.push_str(groups[digit - 1]);
                        }
                        i += 2;
                        continue;
                    }
                    _ => {
                        // Other escaped characters - output the char after backslash
                        result.push(next);
                        i += 2;
                        continue;
                    }
                }
            }
        }

        if chars[i] == '&' {
            result.push_str(match_text);
            i += 1;
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Get the address field from a SedCmd variant.
fn get_address(cmd: &SedCmd) -> &Option<AddressRange> {
    match cmd {
        SedCmd::Substitute { address, .. } => address,
        SedCmd::Print { address } => address,
        SedCmd::PrintFirstLine { address } => address,
        SedCmd::Delete { address } => address,
        SedCmd::DeleteFirstLine { address } => address,
        SedCmd::Append { address, .. } => address,
        SedCmd::Insert { address, .. } => address,
        SedCmd::Change { address, .. } => address,
        SedCmd::Hold { address } => address,
        SedCmd::HoldAppend { address } => address,
        SedCmd::Get { address } => address,
        SedCmd::GetAppend { address } => address,
        SedCmd::Exchange { address } => address,
        SedCmd::Next { address } => address,
        SedCmd::NextAppend { address } => address,
        SedCmd::Quit { address } => address,
        SedCmd::QuitSilent { address } => address,
        SedCmd::Transliterate { address, .. } => address,
        SedCmd::LineNumber { address } => address,
        SedCmd::Branch { address, .. } => address,
        SedCmd::BranchOnSubst { address, .. } => address,
        SedCmd::BranchOnNoSubst { address, .. } => address,
        SedCmd::Zap { address } => address,
        SedCmd::Group { address, .. } => address,
        SedCmd::List { address } => address,
        SedCmd::PrintFilename { address } => address,
        SedCmd::Version { address, .. } => address,
        SedCmd::ReadFile { address, .. } => address,
        SedCmd::ReadFileLine { address, .. } => address,
        SedCmd::WriteFile { address, .. } => address,
        SedCmd::WriteFirstLine { address, .. } => address,
        SedCmd::Label { .. } => {
            // Labels don't have addresses; use a static None
            static NONE: Option<AddressRange> = None;
            &NONE
        }
    }
}

/// Execute a single sed command (non-branching, non-group).
fn execute_command(cmd: &SedCmd, state: &mut SedState) {
    // Labels are handled separately
    if matches!(cmd, SedCmd::Label { .. }) {
        return;
    }

    // Check if command applies to current line
    let address = get_address(cmd).clone();
    if !is_in_range(
        &address,
        state.line_number,
        state.total_lines,
        &state.pattern_space.clone(),
        state,
    ) {
        return;
    }

    match cmd {
        SedCmd::Substitute {
            pattern,
            replacement,
            global,
            ignore_case,
            print_on_match,
            nth_occurrence,
            extended_regex,
            ..
        } => {
            // Handle empty pattern (reuse last pattern)
            let raw_pattern = if pattern.is_empty() {
                if let Some(ref last) = state.last_pattern {
                    last.clone()
                } else {
                    return;
                }
            } else {
                state.last_pattern = Some(pattern.clone());
                pattern.clone()
            };

            // Convert BRE to ERE if not using extended regex mode
            let converted = if *extended_regex {
                raw_pattern.clone()
            } else {
                bre_to_ere(&raw_pattern)
            };
            let normalized = normalize_for_rust(&converted);

            // Build regex pattern with case insensitive flag
            let regex_pattern = if *ignore_case {
                format!("(?i){}", normalized)
            } else {
                normalized
            };

            let re = match Regex::new(&regex_pattern) {
                Ok(r) => r,
                Err(_) => return, // Invalid regex, skip
            };

            // Check if pattern matches FIRST - for t/T command tracking
            let has_match = re.is_match(&state.pattern_space);

            if has_match {
                state.substitution_made = true;
                let replacement = replacement.clone();

                if let Some(nth) = nth_occurrence {
                    if *nth > 0 && !global {
                        // Handle Nth occurrence
                        let nth_val = *nth;
                        let mut count = 0;
                        let ps = state.pattern_space.clone();
                        let mut result = String::new();
                        let mut last_end = 0;

                        for caps in re.captures_iter(&ps) {
                            count += 1;
                            let m = caps.get(0).unwrap();
                            if count == nth_val {
                                result.push_str(&ps[last_end..m.start()]);
                                let groups: Vec<&str> = (1..caps.len())
                                    .map(|i| caps.get(i).map_or("", |m| m.as_str()))
                                    .collect();
                                result.push_str(&process_replacement(
                                    &replacement,
                                    m.as_str(),
                                    &groups,
                                ));
                                last_end = m.end();
                            } else if count > nth_val {
                                // For nth occurrence, we only replace the nth one
                                // but we need to keep scanning to build the result
                                result.push_str(&ps[last_end..m.start()]);
                                result.push_str(m.as_str());
                                last_end = m.end();
                            }
                        }
                        result.push_str(&ps[last_end..]);
                        // If we found fewer than nth occurrences, keep original
                        if count >= nth_val {
                            state.pattern_space = result;
                        }
                    }
                } else if *global {
                    // Use custom global replace for POSIX-compliant zero-length match handling
                    state.pattern_space = global_replace(
                        &state.pattern_space,
                        &re,
                        |matched, groups| process_replacement(&replacement, matched, groups),
                    );
                } else {
                    // Single replacement
                    let ps = state.pattern_space.clone();
                    if let Some(caps) = re.captures(&ps) {
                        let m = caps.get(0).unwrap();
                        let groups: Vec<&str> = (1..caps.len())
                            .map(|i| caps.get(i).map_or("", |m| m.as_str()))
                            .collect();
                        let mut result = String::new();
                        result.push_str(&ps[..m.start()]);
                        result.push_str(&process_replacement(
                            &replacement,
                            m.as_str(),
                            &groups,
                        ));
                        result.push_str(&ps[m.end()..]);
                        state.pattern_space = result;
                    }
                }

                if *print_on_match {
                    let ps = state.pattern_space.clone();
                    state.line_number_output.push(ps);
                }
            }
        }

        SedCmd::Print { .. } => {
            let ps = state.pattern_space.clone();
            state.line_number_output.push(ps);
        }

        SedCmd::PrintFirstLine { .. } => {
            let ps = state.pattern_space.clone();
            if let Some(idx) = ps.find('\n') {
                state.line_number_output.push(ps[..idx].to_string());
            } else {
                state.line_number_output.push(ps);
            }
        }

        SedCmd::Delete { .. } => {
            state.deleted = true;
        }

        SedCmd::DeleteFirstLine { .. } => {
            let ps = state.pattern_space.clone();
            if let Some(idx) = ps.find('\n') {
                state.pattern_space = ps[idx + 1..].to_string();
                state.restart_cycle = true;
                state.in_d_restarted_cycle = true;
            } else {
                state.deleted = true;
            }
        }

        SedCmd::Zap { .. } => {
            state.pattern_space = String::new();
        }

        SedCmd::Append { text, .. } => {
            state.append_buffer.push(text.clone());
        }

        SedCmd::Insert { text, .. } => {
            state.append_buffer.insert(0, format!("__INSERT__{}", text));
        }

        SedCmd::Change { text, .. } => {
            state.deleted = true;
            state.changed_text = Some(text.clone());
        }

        SedCmd::Hold { .. } => {
            state.hold_space = state.pattern_space.clone();
        }

        SedCmd::HoldAppend { .. } => {
            let ps = state.pattern_space.clone();
            if !state.hold_space.is_empty() {
                state.hold_space.push('\n');
                state.hold_space.push_str(&ps);
            } else {
                state.hold_space = ps;
            }
        }

        SedCmd::Get { .. } => {
            state.pattern_space = state.hold_space.clone();
        }

        SedCmd::GetAppend { .. } => {
            let hs = state.hold_space.clone();
            state.pattern_space.push('\n');
            state.pattern_space.push_str(&hs);
        }

        SedCmd::Exchange { .. } => {
            std::mem::swap(&mut state.pattern_space, &mut state.hold_space);
        }

        SedCmd::Next { .. } => {
            // n - handled in execute_commands; here just set printed flag
            state.printed = true;
        }

        SedCmd::Quit { .. } => {
            state.quit = true;
        }

        SedCmd::QuitSilent { .. } => {
            state.quit = true;
            state.quit_silent = true;
        }

        SedCmd::List { .. } => {
            let escaped = escape_for_list(&state.pattern_space);
            state.line_number_output.push(escaped);
        }

        SedCmd::PrintFilename { .. } => {
            if let Some(ref filename) = state.current_filename {
                state.line_number_output.push(filename.clone());
            }
        }

        SedCmd::Version { min_version, .. } => {
            let our_version: [i32; 3] = [4, 8, 0];
            if let Some(ref ver_str) = min_version {
                let parts: Vec<&str> = ver_str.split('.').collect();
                let mut requested: Vec<i32> = Vec::new();
                let mut parse_error = false;
                for part in &parts {
                    match part.parse::<i32>() {
                        Ok(n) if n >= 0 => requested.push(n),
                        _ => {
                            state.quit = true;
                            state.exit_code = Some(1);
                            state.error_message =
                                Some(format!("sed: invalid version string: {}", ver_str));
                            parse_error = true;
                            break;
                        }
                    }
                }
                if !parse_error {
                    while requested.len() < 3 {
                        requested.push(0);
                    }
                    for i in 0..3 {
                        if requested[i] > our_version[i] {
                            state.quit = true;
                            state.exit_code = Some(1);
                            state.error_message =
                                Some(format!("sed: this is not GNU sed version {}", ver_str));
                            break;
                        }
                        if requested[i] < our_version[i] {
                            break;
                        }
                    }
                }
            }
        }

        SedCmd::ReadFile { filename, .. } => {
            state.pending_file_reads.push(PendingFileRead {
                filename: filename.clone(),
                whole_file: true,
            });
        }

        SedCmd::ReadFileLine { filename, .. } => {
            state.pending_file_reads.push(PendingFileRead {
                filename: filename.clone(),
                whole_file: false,
            });
        }

        SedCmd::WriteFile { filename, .. } => {
            let content = format!("{}\n", state.pattern_space);
            state.pending_file_writes.push(PendingFileWrite {
                filename: filename.clone(),
                content,
            });
        }

        SedCmd::WriteFirstLine { filename, .. } => {
            let ps = state.pattern_space.clone();
            let first_line = if let Some(idx) = ps.find('\n') {
                &ps[..idx]
            } else {
                &ps
            };
            state.pending_file_writes.push(PendingFileWrite {
                filename: filename.clone(),
                content: format!("{}\n", first_line),
            });
        }

        SedCmd::Transliterate { source, dest, .. } => {
            let src_chars: Vec<char> = source.chars().collect();
            let dst_chars: Vec<char> = dest.chars().collect();
            let ps = state.pattern_space.clone();
            let mut result = String::new();
            for ch in ps.chars() {
                if let Some(idx) = src_chars.iter().position(|&c| c == ch) {
                    if idx < dst_chars.len() {
                        result.push(dst_chars[idx]);
                    } else {
                        result.push(ch);
                    }
                } else {
                    result.push(ch);
                }
            }
            state.pattern_space = result;
        }

        SedCmd::LineNumber { .. } => {
            state.line_number_output.push(state.line_number.to_string());
        }

        // These are handled in execute_commands
        SedCmd::Branch { .. }
        | SedCmd::BranchOnSubst { .. }
        | SedCmd::BranchOnNoSubst { .. }
        | SedCmd::Group { .. }
        | SedCmd::NextAppend { .. }
        | SedCmd::Label { .. } => {}
    }
}

/// Main execution loop for sed commands.
/// Builds a label index, iterates through commands, handles branching,
/// n/N commands, and grouped commands.
/// Returns lines consumed in cycle.
pub fn execute_commands(
    commands: &[SedCmd],
    state: &mut SedState,
    ctx: &mut ExecuteContext,
) -> usize {
    // Build label index for branching
    let mut label_index: HashMap<String, usize> = HashMap::new();
    for (idx, cmd) in commands.iter().enumerate() {
        if let SedCmd::Label { name } = cmd {
            label_index.insert(name.clone(), idx);
        }
    }

    let mut total_iterations: usize = 0;
    let mut i: usize = 0;

    while i < commands.len() {
        total_iterations += 1;
        if total_iterations > DEFAULT_MAX_ITERATIONS {
            // Prevent infinite loops - just break
            break;
        }

        if state.deleted || state.quit || state.quit_silent || state.restart_cycle {
            break;
        }

        let cmd = &commands[i];

        // Handle n command specially - it needs to print and read next line inline
        if let SedCmd::Next { address } = cmd {
            let addr = address.clone();
            if is_in_range(
                &addr,
                state.line_number,
                state.total_lines,
                &state.pattern_space.clone(),
                state,
            ) {
                let ps = state.pattern_space.clone();
                state.n_command_output.push(ps);

                let next_idx = ctx.current_line_index + state.lines_consumed_in_cycle + 1;
                if next_idx < ctx.lines.len() {
                    state.lines_consumed_in_cycle += 1;
                    let next_line = ctx.lines
                        [ctx.current_line_index + state.lines_consumed_in_cycle]
                        .clone();
                    state.pattern_space = next_line;
                    state.line_number =
                        ctx.current_line_index + state.lines_consumed_in_cycle + 1;
                    state.substitution_made = false;
                } else {
                    state.quit = true;
                    state.deleted = true;
                    break;
                }
            }
            i += 1;
            continue;
        }

        // Handle N command specially - it needs to append next line inline
        if let SedCmd::NextAppend { address } = cmd {
            let addr = address.clone();
            if is_in_range(
                &addr,
                state.line_number,
                state.total_lines,
                &state.pattern_space.clone(),
                state,
            ) {
                let next_idx = ctx.current_line_index + state.lines_consumed_in_cycle + 1;
                if next_idx < ctx.lines.len() {
                    state.lines_consumed_in_cycle += 1;
                    let next_line = ctx.lines
                        [ctx.current_line_index + state.lines_consumed_in_cycle]
                        .clone();
                    state.pattern_space.push('\n');
                    state.pattern_space.push_str(&next_line);
                    state.line_number =
                        ctx.current_line_index + state.lines_consumed_in_cycle + 1;
                } else {
                    state.quit = true;
                    break;
                }
            }
            i += 1;
            continue;
        }

        // Handle branching commands specially
        if let SedCmd::Branch { address, label } = cmd {
            let addr = address.clone();
            if is_in_range(
                &addr,
                state.line_number,
                state.total_lines,
                &state.pattern_space.clone(),
                state,
            ) {
                if let Some(ref lbl) = label {
                    if let Some(&target) = label_index.get(lbl) {
                        i = target;
                        continue;
                    }
                    // Label not found in current scope - request outer scope
                    state.branch_request = Some(lbl.clone());
                    break;
                }
                // Branch without label means jump to end
                break;
            }
            i += 1;
            continue;
        }

        if let SedCmd::BranchOnSubst { address, label } = cmd {
            let addr = address.clone();
            if is_in_range(
                &addr,
                state.line_number,
                state.total_lines,
                &state.pattern_space.clone(),
                state,
            ) {
                if state.substitution_made {
                    state.substitution_made = false;
                    if let Some(ref lbl) = label {
                        if let Some(&target) = label_index.get(lbl) {
                            i = target;
                            continue;
                        }
                        state.branch_request = Some(lbl.clone());
                        break;
                    }
                    break;
                }
            }
            i += 1;
            continue;
        }

        if let SedCmd::BranchOnNoSubst { address, label } = cmd {
            let addr = address.clone();
            if is_in_range(
                &addr,
                state.line_number,
                state.total_lines,
                &state.pattern_space.clone(),
                state,
            ) {
                if !state.substitution_made {
                    if let Some(ref lbl) = label {
                        if let Some(&target) = label_index.get(lbl) {
                            i = target;
                            continue;
                        }
                        state.branch_request = Some(lbl.clone());
                        break;
                    }
                    break;
                }
            }
            i += 1;
            continue;
        }

        // Grouped commands - execute recursively
        if let SedCmd::Group { address, commands: group_cmds } = cmd {
            let addr = address.clone();
            if is_in_range(
                &addr,
                state.line_number,
                state.total_lines,
                &state.pattern_space.clone(),
                state,
            ) {
                execute_commands(group_cmds, state, ctx);

                // Handle cross-group branch request from nested group
                if let Some(ref lbl) = state.branch_request.clone() {
                    if let Some(&target) = label_index.get(lbl) {
                        state.branch_request = None;
                        i = target;
                        continue;
                    }
                    // Label not found in this scope either - propagate up
                    break;
                }
            }
            i += 1;
            continue;
        }

        execute_command(cmd, state);
        i += 1;
    }

    state.lines_consumed_in_cycle
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state(pattern: &str, total: usize) -> SedState {
        let mut state = SedState::new(total);
        state.pattern_space = pattern.to_string();
        state.line_number = 1;
        state
    }

    fn make_ctx(lines: Vec<&str>) -> ExecuteContext {
        ExecuteContext {
            lines: lines.into_iter().map(String::from).collect(),
            current_line_index: 0,
        }
    }

    #[test]
    fn test_substitute_basic() {
        let mut state = make_state("hello world", 1);
        let mut ctx = make_ctx(vec!["hello world"]);
        let cmds = vec![SedCmd::Substitute {
            address: None,
            pattern: "world".to_string(),
            replacement: "rust".to_string(),
            global: false,
            ignore_case: false,
            print_on_match: false,
            nth_occurrence: None,
            extended_regex: false,
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.pattern_space, "hello rust");
    }

    #[test]
    fn test_substitute_global() {
        let mut state = make_state("aaa", 1);
        let mut ctx = make_ctx(vec!["aaa"]);
        let cmds = vec![SedCmd::Substitute {
            address: None,
            pattern: "a".to_string(),
            replacement: "b".to_string(),
            global: true,
            ignore_case: false,
            print_on_match: false,
            nth_occurrence: None,
            extended_regex: false,
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.pattern_space, "bbb");
    }

    #[test]
    fn test_substitute_case_insensitive() {
        let mut state = make_state("Hello World", 1);
        let mut ctx = make_ctx(vec!["Hello World"]);
        let cmds = vec![SedCmd::Substitute {
            address: None,
            pattern: "hello".to_string(),
            replacement: "hi".to_string(),
            global: false,
            ignore_case: true,
            print_on_match: false,
            nth_occurrence: None,
            extended_regex: false,
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.pattern_space, "hi World");
    }

    #[test]
    fn test_delete_command() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Delete { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.deleted);
    }

    #[test]
    fn test_print_command() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Print { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.line_number_output, vec!["hello"]);
    }

    #[test]
    fn test_hold_space_operations() {
        let mut state = make_state("first", 2);
        let mut ctx = make_ctx(vec!["first", "second"]);

        // h - copy to hold
        let cmds = vec![SedCmd::Hold { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.hold_space, "first");

        // Change pattern space
        state.pattern_space = "second".to_string();

        // G - append hold to pattern
        let cmds = vec![SedCmd::GetAppend { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.pattern_space, "second\nfirst");
    }

    #[test]
    fn test_transliterate() {
        let mut state = make_state("abc", 1);
        let mut ctx = make_ctx(vec!["abc"]);
        let cmds = vec![SedCmd::Transliterate {
            address: None,
            source: "abc".to_string(),
            dest: "xyz".to_string(),
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.pattern_space, "xyz");
    }

    #[test]
    fn test_line_number() {
        let mut state = make_state("hello", 5);
        state.line_number = 3;
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::LineNumber { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.line_number_output, vec!["3"]);
    }

    #[test]
    fn test_quit_command() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Quit { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.quit);
        assert!(!state.quit_silent);
    }

    #[test]
    fn test_quit_silent_command() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::QuitSilent { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.quit);
        assert!(state.quit_silent);
    }

    #[test]
    fn test_zap_command() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Zap { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.pattern_space, "");
    }

    #[test]
    fn test_list_command() {
        let mut state = make_state("a\tb", 1);
        let mut ctx = make_ctx(vec!["a\tb"]);
        let cmds = vec![SedCmd::List { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.line_number_output[0].contains("\\t"));
    }

    #[test]
    fn test_address_line_match() {
        let mut state = make_state("hello", 3);
        state.line_number = 2;
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Delete {
            address: Some(AddressRange {
                start: Some(SedAddress::Line(2)),
                end: None,
                negated: false,
            }),
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.deleted);
    }

    #[test]
    fn test_address_line_no_match() {
        let mut state = make_state("hello", 3);
        state.line_number = 1;
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Delete {
            address: Some(AddressRange {
                start: Some(SedAddress::Line(2)),
                end: None,
                negated: false,
            }),
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(!state.deleted);
    }

    #[test]
    fn test_negated_address() {
        let mut state = make_state("hello", 3);
        state.line_number = 1;
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Delete {
            address: Some(AddressRange {
                start: Some(SedAddress::Line(2)),
                end: None,
                negated: true,
            }),
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.deleted); // Line 1 != 2, negated, so delete
    }

    #[test]
    fn test_branch_unconditional() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![
            SedCmd::Label { name: "skip".to_string() },
            SedCmd::Branch { address: None, label: Some("end".to_string()) },
            SedCmd::Delete { address: None },
            SedCmd::Label { name: "end".to_string() },
        ];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(!state.deleted); // Delete was skipped
    }

    #[test]
    fn test_branch_on_subst() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![
            SedCmd::Substitute {
                address: None,
                pattern: "hello".to_string(),
                replacement: "world".to_string(),
                global: false,
                ignore_case: false,
                print_on_match: false,
                nth_occurrence: None,
                extended_regex: false,
            },
            SedCmd::BranchOnSubst { address: None, label: Some("end".to_string()) },
            SedCmd::Delete { address: None },
            SedCmd::Label { name: "end".to_string() },
        ];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(!state.deleted); // Delete was skipped because subst succeeded
    }

    #[test]
    fn test_grouped_commands() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Group {
            address: None,
            commands: vec![
                SedCmd::Print { address: None },
                SedCmd::Delete { address: None },
            ],
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.line_number_output, vec!["hello"]);
        assert!(state.deleted);
    }

    #[test]
    fn test_append_command() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Append {
            address: None,
            text: "world".to_string(),
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.append_buffer, vec!["world"]);
    }

    #[test]
    fn test_insert_command() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Insert {
            address: None,
            text: "world".to_string(),
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.append_buffer[0].starts_with("__INSERT__"));
    }

    #[test]
    fn test_change_command() {
        let mut state = make_state("hello", 1);
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Change {
            address: None,
            text: "world".to_string(),
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.deleted);
        assert_eq!(state.changed_text, Some("world".to_string()));
    }

    #[test]
    fn test_process_replacement_ampersand() {
        let result = process_replacement("(&)", "foo", &[]);
        assert_eq!(result, "(foo)");
    }

    #[test]
    fn test_process_replacement_backreference() {
        let result = process_replacement(r"[\1]", "foo", &["bar"]);
        assert_eq!(result, "[bar]");
    }

    #[test]
    fn test_process_replacement_escape_sequences() {
        let result = process_replacement(r"a\tb\nc", "x", &[]);
        assert_eq!(result, "a\tb\nc");
    }

    #[test]
    fn test_exchange_command() {
        let mut state = make_state("pattern", 1);
        state.hold_space = "hold".to_string();
        let mut ctx = make_ctx(vec!["pattern"]);
        let cmds = vec![SedCmd::Exchange { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.pattern_space, "hold");
        assert_eq!(state.hold_space, "pattern");
    }

    #[test]
    fn test_print_first_line() {
        let mut state = make_state("first\nsecond", 1);
        let mut ctx = make_ctx(vec!["first\nsecond"]);
        let cmds = vec![SedCmd::PrintFirstLine { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.line_number_output, vec!["first"]);
    }

    #[test]
    fn test_delete_first_line() {
        let mut state = make_state("first\nsecond", 1);
        let mut ctx = make_ctx(vec!["first\nsecond"]);
        let cmds = vec![SedCmd::DeleteFirstLine { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.pattern_space, "second");
        assert!(state.restart_cycle);
    }

    #[test]
    fn test_last_address() {
        let mut state = make_state("hello", 5);
        state.line_number = 5;
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Delete {
            address: Some(AddressRange {
                start: Some(SedAddress::Last),
                end: None,
                negated: false,
            }),
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.deleted);
    }

    #[test]
    fn test_pattern_address() {
        let mut state = make_state("hello world", 1);
        let mut ctx = make_ctx(vec!["hello world"]);
        let cmds = vec![SedCmd::Delete {
            address: Some(AddressRange {
                start: Some(SedAddress::Pattern("world".to_string())),
                end: None,
                negated: false,
            }),
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.deleted);
    }

    #[test]
    fn test_step_address() {
        // Step 0~2 matches even lines: 0, 2, 4, ...
        let mut state = make_state("hello", 10);
        state.line_number = 4;
        let mut ctx = make_ctx(vec!["hello"]);
        let cmds = vec![SedCmd::Delete {
            address: Some(AddressRange {
                start: Some(SedAddress::Step { first: 0, step: 2 }),
                end: None,
                negated: false,
            }),
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert!(state.deleted);
    }

    #[test]
    fn test_create_initial_state() {
        let mut range_states = HashMap::new();
        range_states.insert("test".to_string(), RangeState::default());
        let state = create_initial_state(10, Some("test.txt"), range_states);
        assert_eq!(state.total_lines, 10);
        assert_eq!(state.current_filename, Some("test.txt".to_string()));
        assert!(state.range_states.contains_key("test"));
        assert_eq!(state.pattern_space, "");
        assert_eq!(state.line_number, 0);
    }

    #[test]
    fn test_hold_append() {
        let mut state = make_state("first", 2);
        state.hold_space = "existing".to_string();
        let mut ctx = make_ctx(vec!["first"]);
        let cmds = vec![SedCmd::HoldAppend { address: None }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.hold_space, "existing\nfirst");
    }

    #[test]
    fn test_substitute_print_on_match() {
        let mut state = make_state("hello world", 1);
        let mut ctx = make_ctx(vec!["hello world"]);
        let cmds = vec![SedCmd::Substitute {
            address: None,
            pattern: "world".to_string(),
            replacement: "rust".to_string(),
            global: false,
            ignore_case: false,
            print_on_match: true,
            nth_occurrence: None,
            extended_regex: false,
        }];
        execute_commands(&cmds, &mut state, &mut ctx);
        assert_eq!(state.pattern_space, "hello rust");
        assert_eq!(state.line_number_output, vec!["hello rust"]);
    }
}
