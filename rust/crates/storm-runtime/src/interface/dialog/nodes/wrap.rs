pub(super) fn lines(label: &str, width: i32, measure: impl Fn(&str) -> i32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = label;
    loop {
        let mut line = String::new();
        let mut previous_space = None;
        while let Some(ch) = rest.chars().next() {
            if ch == ' ' {
                if measure(&line) >= width {
                    break;
                }
                previous_space = Some((rest, line.len()));
                rest = rest.trim_start_matches(' ');
                line.push(' ');
                continue;
            }
            if rest.starts_with("\\n") || rest.starts_with("\\N") {
                break;
            }
            line.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
        if measure(&line) > width
            && let Some((remainder, length)) = previous_space
        {
            rest = remainder;
            line.truncate(length);
            line.push(' ');
        }
        rest = rest.trim_start_matches(' ');
        if let Some(after) = rest
            .strip_prefix("\\n")
            .or_else(|| rest.strip_prefix("\\N"))
        {
            rest = after;
            line.truncate(line.trim_end_matches(' ').len());
        }
        lines.push(line);
        if rest.is_empty() {
            return lines;
        }
    }
}
