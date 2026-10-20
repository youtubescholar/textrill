// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Turning an engine failure message into a list of actionable problems
//! (roadmap §7, workstream 4).
//!
//! The engine reports one problem per line. Option-file errors carry a
//! `file:line:` prefix; content problems are plain sentences. Nothing here
//! invents a location: a problem is clickable only when the text names one.

/// One problem from the last failed conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    /// The message exactly as the engine wrote it.
    pub message: String,
    /// The 1-based source line named by `message`, if it names one.
    pub line: Option<usize>,
}

/// Split an engine failure message into problems, one per non-empty line.
pub fn parse(text: &str) -> Vec<Problem> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|message| Problem {
            message: message.to_string(),
            line: line_of(message),
        })
        .collect()
}

/// The line a message points at: `label:12: …` (option files) or
/// `line 12: …` / `line 12` (content). An absent or zero line is `None`.
fn line_of(message: &str) -> Option<usize> {
    if let Some(rest) = message.strip_prefix("line ") {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(line) = digits.parse::<usize>() {
            if line > 0 {
                return Some(line);
            }
        }
    }
    // `path/to/options.txt:12: message`. The label may itself contain colons
    // on some platforms, so try every `:` from the left and take the first
    // segment that is a positive integer followed by another `:`.
    for (index, _) in message.match_indices(':') {
        let rest = &message[index + 1..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() {
            continue;
        }
        if let Some(after) = rest[digits.len()..].strip_prefix(':') {
            if let Ok(line) = digits.parse::<usize>() {
                if line > 0 && !after.is_empty() {
                    return Some(line);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_file_errors_name_their_line() {
        let problems = parse("/home/me/.textrillrc:12: unknown option `wat`");
        assert_eq!(
            problems,
            vec![Problem {
                message: "/home/me/.textrillrc:12: unknown option `wat`".to_string(),
                line: Some(12),
            }]
        );
    }

    #[test]
    fn a_line_prefix_is_understood_with_and_without_a_colon() {
        assert_eq!(line_of("line 7: bad marker"), Some(7));
        assert_eq!(line_of("line 7"), Some(7));
        assert_eq!(line_of("line zero"), None);
    }

    #[test]
    fn plain_sentences_have_no_location() {
        let problems = parse("reference to \"x\" has no definition (Citation)\n");
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].line, None);
    }

    #[test]
    fn blank_lines_are_not_problems() {
        assert!(parse("\n\n  \n").is_empty());
    }

    #[test]
    fn a_clock_time_is_not_a_line_number() {
        assert_eq!(line_of("finished at 12:30"), None);
    }
}
