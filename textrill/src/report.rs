//! `--report`: what the conversion recovered, on standard error.
//!
//! Counts are of the tags in the **produced output**, not of events the engine
//! believes it performed, so a user with the file and `grep` can reproduce
//! every one of them. Counting at emission sites would not survive passes that
//! rewrite the output, e.g. `notes.rs` truncating an empty `<p>` wrapper.
//! Covered by tests/reporttest.rs.

/// The five numbers `--report` prints, for one document or one run's output.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Output size in bytes — what `wc -c` reports.
    pub bytes: usize,
    /// `<h1>` … `<h6>` open tags, in either tag case.
    pub headings: usize,
    /// Paragraph open tags: `<p>`, `<p class="…">`, `<P>`. `<pre>` is not one.
    pub paragraphs: usize,
    /// `<strong>` runs — the capitals and delimiter inference.
    pub strong: usize,
    /// `<br>`, in either serialisation (`<br>`, `<br/>`).
    pub br: usize,
}

impl Counts {
    /// Count what is in `html`, case-insensitively so `--no-html5`'s
    /// upper-case tags count as structure rather than serialisation.
    pub fn of(html: &str) -> Counts {
        let b = html.as_bytes();
        let mut c = Counts {
            bytes: b.len(),
            ..Counts::default()
        };
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'<' {
                // Text is escaped and no UTF-8 continuation byte is '<'.
                let rest = &b[i + 1..];
                if starts_with(rest, b"h") && rest.get(1).is_some_and(|d| (b'1'..=b'6').contains(d))
                {
                    c.headings += 1;
                } else if starts_with(rest, b"p")
                    && rest
                        .get(1)
                        .is_some_and(|d| *d == b'>' || d.is_ascii_whitespace())
                {
                    c.paragraphs += 1;
                } else if starts_with(rest, b"strong") {
                    c.strong += 1;
                } else if starts_with(rest, b"br") {
                    c.br += 1;
                }
            }
            i += 1;
        }
        c
    }

    /// Add another document's counts; `--chunk` sums its per-file totals.
    pub fn add(&mut self, other: &Counts) {
        self.bytes += other.bytes;
        self.headings += other.headings;
        self.paragraphs += other.paragraphs;
        self.strong += other.strong;
        self.br += other.br;
    }
}

impl std::fmt::Display for Counts {
    /// The line `--report` prints, after the `textrill: report ` prefix.
    /// `key=value` so a person can read it and a test can compare it.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bytes={} headings={} paragraphs={} strong={} br={}",
            self.bytes, self.headings, self.paragraphs, self.strong, self.br
        )
    }
}

fn starts_with(hay: &[u8], needle: &[u8]) -> bool {
    hay.len() >= needle.len() && hay[..needle.len()].eq_ignore_ascii_case(needle)
}

#[cfg(test)]
mod tests {
    use super::Counts;

    #[test]
    fn an_empty_document_reports_nothing_but_its_size() {
        assert_eq!(Counts::of(""), Counts::default());
        let html = "<!DOCTYPE html>\n<html>\n<body>\n</body>\n</html>\n";
        let c = Counts::of(html);
        assert_eq!((c.headings, c.paragraphs, c.strong, c.br), (0, 0, 0, 0));
        assert_eq!(c.bytes, html.len());
    }

    #[test]
    fn a_paragraph_counts_once_whatever_it_carries() {
        // `<pre>` shares the `<p` prefix, and mailmode paragraphs carry an
        // attribute.
        assert_eq!(Counts::of("<p>hi\n").paragraphs, 1);
        assert_eq!(Counts::of("<p class='mail_header'>From\n").paragraphs, 1);
        assert_eq!(Counts::of("<p\n>").paragraphs, 1);
        assert_eq!(Counts::of("<pre>\n  x\n</pre>\n").paragraphs, 0);
        assert_eq!(Counts::of("<P>HI\n").paragraphs, 1);
    }

    #[test]
    fn headings_are_the_six_levels_and_nothing_else() {
        let mut html = String::new();
        for n in 1..=6 {
            html.push_str(&format!("<h{n}>t</h{n}>\n"));
        }
        html.push_str("<hr>\n<h7>t</h7>\n");
        assert_eq!(Counts::of(&html).headings, 6);
        assert_eq!(Counts::of("<H1>T</H1>\n").headings, 1);
    }

    #[test]
    fn strong_and_br_count_by_prefix_in_either_case() {
        // The report counts what was written: `--caps_tag b` emits no `<strong>`.
        assert_eq!(Counts::of("<p>a <strong>b</strong>\n").strong, 1);
        assert_eq!(Counts::of("<p>a <b>b</b>\n").strong, 0);
        assert_eq!(Counts::of("<br>\n<br/>\n<BR>\n").br, 3);
    }

    #[test]
    fn escaped_text_is_not_markup() {
        let c = Counts::of("<p>a &lt;p&gt; and &lt;strong&gt;\n");
        assert_eq!((c.paragraphs, c.strong), (1, 0));
    }

    #[test]
    fn counts_add_for_multi_file_runs() {
        let a = "<h1>one</h1>\n<p>a\n";
        let b = "<h1>two</h1>\n<p>b\n<p>c\n";
        let mut total = Counts::of(a);
        total.add(&Counts::of(b));
        assert_eq!((total.headings, total.paragraphs), (2, 3));
        assert_eq!(total.bytes, a.len() + b.len());
    }

    #[test]
    fn the_line_is_the_five_keys_in_order() {
        let c = Counts {
            bytes: 38477,
            headings: 0,
            paragraphs: 64,
            strong: 39,
            br: 34,
        };
        assert_eq!(
            c.to_string(),
            "bytes=38477 headings=0 paragraphs=64 strong=39 br=34"
        );
    }
}
