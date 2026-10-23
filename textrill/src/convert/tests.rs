use super::decode::{chop_leading_cr, chop_trailing_cr};
use super::inline::{
    no_list_or_para_tag, non_boundary, not_preceded_by, not_preceded_by_str,
    not_preceded_by_word_or_underscore,
};
use super::stream::split_blank_lines;
use super::*;

#[cfg(test)]
mod cr_chop_tests {
    use super::{chop_leading_cr, chop_trailing_cr, split_blank_lines};

    /// The patterns the two helpers replaced, compiled as the engine compiled
    /// them. They must go through `ascii_re_cached`, not `Regex::new`, because
    /// that is where Perl's `$` is restored: `$` is rewritten to the lookahead
    /// `(?=\n?$)` so it matches before a trailing newline.
    fn re_trailing() -> std::sync::Arc<fancy_regex::Regex> {
        crate::links::ascii_re_cached(r"[ \t]*\x0D$")
    }
    fn re_leading() -> std::sync::Arc<fancy_regex::Regex> {
        crate::links::ascii_re_cached(r"^[ \t]*\x0D")
    }

    /// Exhaustive over the alphabet that matters. The helpers stand in for a
    /// regex and the failure mode is a silent difference on one awkward input,
    /// so this checks every string up to length 4 rather than a few examples.
    #[test]
    fn matches_the_regex_it_replaced() {
        let alphabet = ['a', ' ', '\t', '\r', '\n'];
        let mut inputs: Vec<String> = vec![String::new()];
        // every string of length 0..=4 over the alphabet
        let mut level = inputs.clone();
        for _ in 0..4 {
            let mut next = Vec::new();
            for s in &level {
                for &c in &alphabet {
                    let mut t = s.clone();
                    t.push(c);
                    next.push(t);
                }
            }
            inputs.extend(next.iter().cloned());
            level = next;
        }
        assert_eq!(inputs.len(), 1 + 5 + 25 + 125 + 625);
        for s in &inputs {
            let want_t = re_trailing().replace(s, "").to_string();
            let want_l = re_leading().replace(s, "").to_string();
            assert_eq!(chop_trailing_cr(s), want_t, "trailing, input {s:?}");
            assert_eq!(chop_leading_cr(s), want_l, "leading, input {s:?}");
        }
    }

    /// `split_blank_lines` stands in for `split(/\r?\n\r?\n/, $s)`; this checks
    /// it over the same alphabet against the regex itself.
    ///
    /// Perl's trailing-empty-field rule is a property of `split`, not of the
    /// pattern, and is easy to get wrong by reading.
    #[test]
    fn split_blank_lines_matches_perl_split() {
        let seps = crate::links::ascii_re_cached(r"\r?\n\r?\n");
        let alphabet = ['a', ' ', '\r', '\n'];
        let mut inputs: Vec<String> = vec![String::new()];
        let mut level = inputs.clone();
        for _ in 0..5 {
            let mut next = Vec::new();
            for s in &level {
                for &c in &alphabet {
                    let mut t = s.clone();
                    t.push(c);
                    next.push(t);
                }
            }
            inputs.extend(next.iter().cloned());
            level = next;
        }
        for s in &inputs {
            // Perl semantics: split on every non-overlapping match, keeping
            // leading and middle empty fields, then drop trailing empties.
            let mut want: Vec<String> = seps
                .split(s)
                .map(|f| {
                    f.unwrap_or_else(|e| panic!("split failed on {s:?}: {e}"))
                        .to_string()
                })
                .collect();
            while want.last().is_some_and(|f| f.is_empty()) {
                want.pop();
            }
            assert_eq!(split_blank_lines(s), want, "input {s:?}");
        }
    }

    /// A record whose only separator is a trailing blank line yields no
    /// paragraph.
    #[test]
    fn a_trailing_blank_line_is_not_a_paragraph() {
        // Expected values produced by running `split(/\r?\n\r?\n/, ...)` under
        // Perl, not by reasoning about the pattern.
        assert_eq!(split_blank_lines("\r\n\r\n"), Vec::<String>::new());
        assert_eq!(split_blank_lines("\n\n"), Vec::<String>::new());
        assert_eq!(split_blank_lines("\r\n\r\n\r\n\r\n"), Vec::<String>::new());
        // a separator at the very end leaves no paragraph behind it
        assert_eq!(split_blank_lines("a\r\n\r\n"), vec!["a"]);
        // ... but an empty field is kept whenever it is not the last one
        assert_eq!(split_blank_lines("\r\n\r\na"), vec!["", "a"]);
        assert_eq!(split_blank_lines("a\r\n\r\nb"), vec!["a", "b"]);
        // only one separator fits in `a\r\n\r\n\r\nb`, so the tail is one field
        assert_eq!(split_blank_lines("a\r\n\r\n\r\nb"), vec!["a", "\r\nb"]);
        assert_eq!(split_blank_lines("\r\n\r\n\r\n"), vec!["", "\r\n"]);
        assert_eq!(split_blank_lines("a\r\n\r\n\r"), vec!["a", "\r"]);
        assert_eq!(
            split_blank_lines("a\r\n\r\na\r\n\r\nb"),
            vec!["a", "a", "b"]
        );
    }

    #[test]
    fn trailing_chop_handles_crlf() {
        // `$` matches before one trailing newline, so a DOS line ending must be
        // recognised; a plain `ends_with('\r')` would miss these.
        assert_eq!(chop_trailing_cr("abc \r"), "abc");
        assert_eq!(chop_trailing_cr("abc \r\n"), "abc\n");
        assert_eq!(chop_trailing_cr("abc\r\n"), "abc\n");
        assert_eq!(chop_trailing_cr("\r"), "");
        assert_eq!(chop_trailing_cr("\r\n"), "\n");
        // no match: left exactly as it was
        assert_eq!(chop_trailing_cr("abc"), "abc");
        assert_eq!(chop_trailing_cr("abc\n"), "abc\n");
        assert_eq!(chop_trailing_cr(" abc "), " abc ");
    }

    #[test]
    fn leading_chop_needs_a_cr() {
        // Trimming leading whitespace unconditionally would destroy every
        // paragraph's indentation.
        assert_eq!(chop_leading_cr("  \tabc"), "  \tabc");
        assert_eq!(chop_leading_cr("  abc"), "  abc");
        assert_eq!(chop_leading_cr("  \rabc"), "abc");
        assert_eq!(chop_leading_cr("  \t\rabc"), "abc");
        assert_eq!(chop_leading_cr("\rabc"), "abc");
        assert_eq!(chop_leading_cr("  \r"), "");
    }
}

/// Differential test for the linear (`delim_replace`) delimiter substitution:
/// the `\B` and `(?<!...)` variants it replaces drove fancy-regex's backtracking
/// VM, which explodes on ~500 KB paragraphs. The linear form must agree.
#[cfg(test)]
mod delim_linear_tests {
    use super::*;
    use crate::links::ascii_re;
    use crate::options::Options;

    fn conv() -> Converter {
        let mut opts = Options::default();
        opts.deal_with_options();
        Converter::new(opts)
    }

    /// Run `body` over every text in `texts`, with one [`Converter`] per worker
    /// and the work split across the machine's cores. The random suites below
    /// are 20,000 single-threaded iterations of two fancy-regex differentials
    /// each, which is minutes of wall clock on one core; the iterations are
    /// independent, so spreading them over the cores costs nothing in coverage
    /// and turns the suite from minutes into seconds.
    ///
    /// It uses *half* the cores, not all of them: `cargo test` runs test
    /// functions in parallel, so the three heavy suites here can be in flight
    /// at once, and each claiming every core would oversubscribe the machine
    /// and run *slower* than half-cores each. Two suites at half the cores fill
    /// the box without the contention; a lone suite still uses half, which is
    /// ample next to the regex work that dominates the runtime.
    ///
    /// Each worker builds its own `Converter` (the checks take `&mut` and the
    /// cached regexes are shared safely), so there is no state to synchronize.
    pub(crate) fn run_parallel(texts: &[String], body: impl Fn(&mut Converter, &str) + Sync) {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        let workers = (cores / 2).max(1);
        if workers <= 1 || texts.len() < 2 {
            let mut c = conv();
            for text in texts {
                body(&mut c, text);
            }
            return;
        }
        let chunk = texts.len().div_ceil(workers);
        std::thread::scope(|scope| {
            for slice in texts.chunks(chunk) {
                let body = &body;
                scope.spawn(move || {
                    let mut c = conv();
                    for text in slice {
                        body(&mut c, text);
                    }
                });
            }
        });
    }

    /// The original `replace_all_captures`: iterate matches with
    /// `captures_iter`.
    fn orig_replace(re: &fancy_regex::Regex, text: &str, tag: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for m in re.captures_iter(text).flatten() {
            if let Some(full) = m.get(0) {
                out.push_str(&text[last..full.start()]);
                out.push_str(&format!("<{tag}>{}</{tag}>", m.get(1).unwrap().as_str()));
                last = full.end();
            }
        }
        out.push_str(&text[last..]);
        out
    }

    pub(crate) fn check_pair(c: &mut Converter, text: &str, delim: char, tag: &str) {
        let db = class_body(&delim.to_string());
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");

        // single alpha char between delims, wrapped in \B assertions
        let re1_orig = ascii_re(&format!(r"\B[{db}]([A-Za-z])[{db}]\B"));
        let re1_lin = ascii_re(&format!(r"[{db}]([A-Za-z])[{db}]"));
        let expect = orig_replace(&re1_orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, &re1_lin, non_boundary(delim), ltag);
        assert_eq!(got, expect, "re1 mismatch for {text:?} delim={delim}");

        // multi-char content ending in a "word/punct" char, leading
        // (?<!delim)
        let re2_orig = ascii_re(&format!(
            r"(?<![{db}])[{db}]([^{db}]+?[A-Za-z0-9!-/:-@\[-`{{-~&<>])[{db}]"
        ));
        let re2_lin = ascii_re(&format!(
            r"[{db}]([^{db}]+?[A-Za-z0-9!-/:-@\[-`{{-~&<>])[{db}]"
        ));
        let expect = orig_replace(&re2_orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, &re2_lin, not_preceded_by(delim), ltag);
        assert_eq!(got, expect, "re2 mismatch for {text:?} delim={delim}");
    }

    #[test]
    fn linear_delim_matches_original_regexes() {
        let cases = [
            "",
            "a",
            "*",
            "**",
            "***",
            "*a*",
            "x*a*",
            "*a*y*",
            " *a* ",
            "**a*",
            "*a**",
            "*a*b*",
            "*a b*",
            "*a!*",
            "a*b*c*",
            "*ab*",
            "*  *",
            "*\t*",
            "*a*b*c*",
            "*a**b*",
            "*a*b**c*",
            "#a#",
            "x#a#",
            "#ab#",
            "**#a#",
            "#a**",
            "#a#b#",
            "_a_",
            "x_a_",
            "_a_b_",
            "__a_",
            "_ab_",
            "_a!_",
            "^a^",
            "x^a^",
            "^a^b^",
            "^^a^",
            "^a^^",
            "a*b_#c^d",
            "*a**",
            "3#4#",
            "_",
            "#",
            "^",
            "*",
            "a",
            " a ",
            "1*2*3",
            "*a*#b#_c_^d^",
            // Non-ASCII delimiter before another: the `(?<!é)` assertion is
            // vacuous byte-wise, since `é` truncates to 0xE9, not one of its
            // bytes, so `ééwordé` was marked up where Perl leaves it alone.
            "ééwordé",
            "üüxü",
            "ééé",
            "üxüyü",
            "xééyéé",
            "ééwördé",
            "éé X üéü",
        ];
        let mut c = conv();
        for text in cases {
            for &(delim, tag) in DELIMS {
                check_pair(&mut c, text, delim, tag);
            }
        }
    }

    /// The delimiters exercised below; the last two are not ASCII, for which
    /// `delim as u8` is silently wrong (`é` truncates to 0xE9, not one of its
    /// bytes), so a preceding-delimiter test built on it never rejects anything.
    pub(crate) const DELIMS: &[(char, &str)] = &[
        ('*', "em"),
        ('#', "strong"),
        ('_', "u"),
        ('^', "em"),
        ('é', "em"),
        ('ü', "strong"),
    ];

    /// Randomized differential: the alphabet is the union of delimiter and
    /// word/punct characters, so it keeps producing the shapes the boundary and
    /// lookbehind conditions care about.
    #[test]
    fn linear_delim_matches_on_random_strings() {
        let alphabet: &[char] = &[
            'a', 'b', 'Z', '0', '9', ' ', '\t', '\n', '!', '.', '-', '_', '#', '*', '^', '<', '>',
            '&', '"', '/', '=', '@', '`', '{', '}', '~',
        ];
        let mut state = 0x243F_6A88_85A3_08D3u64;
        let mut next = || {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let texts: Vec<String> = (0..20_000)
            .map(|_| {
                let len = (next() % 24) as usize;
                (0..len)
                    .map(|_| alphabet[(next() as usize) % alphabet.len()])
                    .collect()
            })
            .collect();
        run_parallel(&texts, |c, text| {
            for &(delim, tag) in DELIMS {
                check_pair(c, text, delim, tag);
            }
        });
    }
}

/// Same differential check for the two remaining `do_delim` rewrites: the bold
/// `#...#` pattern and the underscore pattern.
#[cfg(test)]
mod delim_wide_linear_tests {
    use super::delim_linear_tests::{check_pair, run_parallel};
    use super::*;
    use crate::links::{ascii_re, ascii_re_cached};
    use crate::options::Options;

    fn conv() -> Converter {
        let mut opts = Options::default();
        opts.deal_with_options();
        Converter::new(opts)
    }

    fn orig_replace(re: &fancy_regex::Regex, text: &str, tag: &str) -> String {
        let mut out = String::new();
        let mut last = 0;
        for m in re.captures_iter(text).flatten() {
            if let Some(full) = m.get(0) {
                out.push_str(&text[last..full.start()]);
                out.push_str(&format!("<{tag}>{}</{tag}>", m.get(1).unwrap().as_str()));
                last = full.end();
            }
        }
        out.push_str(&text[last..]);
        out
    }

    fn check_bold(c: &mut Converter, text: &str) {
        let tag = "strong";
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        let orig = ascii_re_cached(r"#([^\d#](?![^#]*(?:<li>|<LI>|<P>|<p>))[^#]*[^# \t\n])#");
        let lin = ascii_re_cached(r"#([^\d#][^#]*[^# \t\n])#");
        let no_tag = no_list_or_para_tag;
        let expect = orig_replace(&orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, &lin, no_tag, ltag);
        assert_eq!(got, expect, "bold re2 mismatch for {text:?}");
    }

    fn check_under(c: &mut Converter, text: &str) {
        let tag = "u";
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        let orig = ascii_re_cached(r#"(?<![_A-Za-z0-9])_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#);
        let lin = ascii_re_cached(r#"_([^_]+?[A-Za-z0-9"'.?&;:<>])_"#);
        let not_word = not_preceded_by_word_or_underscore;
        let expect = orig_replace(&orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, &lin, not_word, ltag);
        assert_eq!(got, expect, "under re2 mismatch for {text:?}");
    }

    #[test]
    fn wide_linear_delim_matches_original_regexes() {
        let cases = [
            "",
            "#",
            "##",
            "###",
            "#a#",
            "x#a#",
            "#a#b#",
            "#ab cd#",
            "#1a#",
            "#a1#",
            "#<p>x#",
            "#x<p>y#",
            "#<li>#",
            "#<LI>x#",
            "#<P>#",
            "# a#",
            "#a #",
            "#a\t#",
            "#a\nb#",
            "##a##",
            "#a# #b#",
            "#<a href=\"#\">#",
            "_",
            "__",
            "_a_",
            "x_a_",
            "_a_b_",
            "__a_",
            "_ab cd_",
            "_a_ _b_",
            "_a b _",
            "_a!_",
            "_a b_",
            "9_a_",
            "a_b_",
            "_a_b_",
            "#x#_y_",
            "_#x#_",
            "#_a_#",
            "***",
            "#a#b#_c_",
            "# #",
            "#<p><p>#",
            "#<li> <P> <p> <LI>#",
            "#\t#",
            "#\n#",
            "#a# #",
            "#5#",
            "# a b #",
        ];
        let mut c = conv();
        for text in cases {
            check_bold(&mut c, text);
            check_under(&mut c, text);
        }
    }

    /// The multi-character delimiter branch, whose leading `(?<!delim)` also
    /// moved into `accept`. `delim` is metachar-free.
    fn check_multi(c: &mut Converter, text: &str, delim: &str) {
        let tag = "em";
        let ltag = |s: &str| format!("<{tag}>{s}</{tag}>");
        let d = fancy_regex::escape(delim);
        let orig = ascii_re(&format!(
            r#"(?<!{d}){d}((\w|["'])(\w|[-\s!-/:-@\[-`{{-~])*[^\s]){d}"#
        ));
        let lin = ascii_re(&format!(
            r#"{d}((\w|["'])(\w|[-\s!-/:-@\[-`{{-~])*[^\s]){d}"#
        ));
        let expect = orig_replace(&orig, text, tag);
        let mut got = text.to_string();
        c.delim_replace(&mut got, &lin, not_preceded_by_str(d.to_string()), ltag);
        assert_eq!(got, expect, "multi mismatch for {text:?} delim={delim}");
    }

    /// `delim_loop` must behave like the regex it replaces when a candidate is
    /// rejected: the engine retries one character along, so a match starting
    /// *inside* the rejected span is still found. Regression test for the
    /// `#`/`</p><p>` case.
    #[test]
    fn wide_delim_retry_finds_match_inside_rejected_span() {
        let cases = [
            "#a</p><p>b#c#",
            "#a<p>b#c#",
            "#<li>x#y#",
            "#<P>x#y#",
            "#<p>#y#",
            "pre #a</p><p>b# mid #c#",
            "#a# #<p>b#c#",
            "_a</p><p>b_c_d_",
            "#<p>only#",
            "#<p>#",
            "##<p>x#y#",
            "#no close at all",
            "_a</p><p>b_c_d_e_",
        ];
        let mut c = conv();
        for text in cases {
            // bold path, including the in-link-context variant that selects
            // `delim_loop` in the first place
            let re = c.re(r"#([^\d#][^#]*[^# \t\n])#").clone();
            let no_tag = no_list_or_para_tag;
            let expect = orig_replace(
                &ascii_re_cached(r"#([^\d#](?![^#]*(?:<li>|<LI>|<P>|<p>))[^#]*[^# \t\n])#"),
                text,
                "strong",
            );
            let got = c.delim_loop(&mut text.to_string(), &re, no_tag, "strong");
            assert_eq!(got, expect, "delim_loop bold mismatch for {text:?}");
        }
    }

    /// 8-bit characters: a regex match position can land in the middle of a
    /// multi-byte character, so any code indexing a `str` by a match offset must
    /// cope. Regression test for the "start byte index N is not a char boundary"
    /// panic on the reference's own `umlauttest` fixture.
    #[test]
    fn linear_delim_handles_multibyte_text() {
        let cases = [
            "ver\u{e4}ndern *ver\u{e4}ndern* zu",
            "#ver\u{e4}ndern# #\u{c4}NDERN# #a\u{fc}b#",
            "_geht_ _ver\u{e4}ndern_ _\u{c4}NDERN_",
            "#a\u{e4}b#<p>c#",
            "#a\u{e4}<p>b#",
            "#a\u{e4}<li>b#",
            "#a\u{20ac}b# #a\u{2192}b# #\u{1f600}#",
            "*\u{e4}* *\u{1f600}x*",
            "_\u{e4}_ _a\u{e4}b_",
            "\u{e4}#b#\u{e4}",
            "#\u{e4}b\u{e4}#",
            "#\u{e4}#",
            "@@\u{e4}b@@",
            "XY\u{e4}XY",
            "@@a\u{e4}@@",
            "XYa\u{e4}XY",
            "#\u{4e2d}\u{6587}#",
            "_\u{4e2d}\u{6587}_",
            "#a#\u{4e2d}#b#",
        ];
        let mut c = conv();
        for text in cases {
            check_bold(&mut c, text);
            check_under(&mut c, text);
            for delim in ["XY", "@@"] {
                check_multi(&mut c, text, delim);
            }
        }
    }

    /// Same, at random: multi-byte characters mixed with delimiters, so match
    /// offsets land off character boundaries often.
    #[test]
    fn linear_delim_handles_multibyte_random() {
        let alphabet: &[char] = &[
            'a', 'b', '0', ' ', '_', '#', '*', '^', 'X', 'Y', '@', '.', '\u{e4}', '\u{fc}',
            '\u{c4}', '\u{20ac}', '\u{4e2d}', '\u{2192}', '\n', '\t',
        ];
        // a delimiter is 0xC3 0xA9, which shares its trailing byte 0xA9 with
        // several other characters, so a byte-wise "previous char" test is wrong
        const NONASCII: &[(char, &str)] = &[('\u{e9}', "em"), ('\u{fc}', "strong")];
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let texts: Vec<String> = (0..20_000)
            .map(|_| {
                let len = (next() % 24) as usize;
                (0..len)
                    .map(|_| alphabet[(next() as usize) % alphabet.len()])
                    .collect()
            })
            .collect();
        run_parallel(&texts, |c, text| {
            check_bold(c, text);
            check_under(c, text);
            for delim in ["XY", "@@"] {
                check_multi(c, text, delim);
            }
            for (delim, tag) in NONASCII {
                check_pair(c, text, *delim, tag);
            }
        });
    }

    #[test]
    fn multi_linear_delim_matches_original_regex() {
        let cases = [
            "",
            "XY",
            "XYXY",
            "XYwordXY",
            "aXYwordXYb",
            "XYword XY",
            "XYword ",
            "XYword",
            "XY w XY",
            "XYwordXYXYwordXY",
            "xXYwordXY",
            "XYwordXYx",
            "@@w@@",
            "w@@w@@",
            "a@@b@@",
            "@@ab@@",
            "@@a b@@",
            "@@@@",
            "@@a@@b@@",
            "XY''XY",
            "XY'a'bXY",
            "XY\"x\"XY",
            "XY\"XY",
            "**bold**",
            "XYaXYbXY",
            " XYwordXY ",
            "\nXYwordXY\n",
            "XY\tXY",
            "@@1@@",
            "@@a1@@",
            "@@a!@@",
        ];
        let mut c = conv();
        for text in cases {
            for delim in ["XY", "@@"] {
                check_multi(&mut c, text, delim);
            }
        }
    }

    #[test]
    fn multi_linear_delim_matches_on_random_strings() {
        let alphabet: &[char] = &[
            'a', 'b', 'Z', '0', ' ', '\t', '\n', '!', '.', '-', '@', 'X', 'Y', '*', '"', '\'', '_',
            '#', '^', '<', '>', '&', '/', '=', '`', '{',
        ];
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let texts: Vec<String> = (0..20_000)
            .map(|_| {
                let len = (next() % 28) as usize;
                (0..len)
                    .map(|_| alphabet[(next() as usize) % alphabet.len()])
                    .collect()
            })
            .collect();
        run_parallel(&texts, |c, text| {
            for delim in ["XY", "@@"] {
                check_multi(c, text, delim);
            }
        });
    }

    #[test]
    fn wide_linear_delim_matches_on_random_strings() {
        let alphabet: &[char] = &[
            'a', 'b', 'Z', '0', '9', ' ', '\t', '\n', '!', '.', '-', '_', '#', '*', '^', '<', '>',
            '&', '"', '/', '=', '@', '`', '{', '}', '~', '?', ';', ':', '\'',
        ];
        let mut state = 0xB502_6F5A_A966_19E9u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let texts: Vec<String> = (0..20_000)
            .map(|_| {
                let len = (next() % 28) as usize;
                (0..len)
                    .map(|_| alphabet[(next() as usize) % alphabet.len()])
                    .collect()
            })
            .collect();
        run_parallel(&texts, |c, text| {
            check_bold(c, text);
            check_under(c, text);
        });
    }
}

#[cfg(test)]
mod re_cache_tests {
    use super::*;
    use crate::options::Options;

    /// The cap, restated here so the test need not reach into the engine for a
    /// constant. `re_cache_bounded_by_a_measured_maximum` asserts the two agree.
    const MAX: usize = 128;

    /// Options set so that each one that feeds a pattern gives a *different*
    /// pattern. Every pattern the engine builds from an option is derived from
    /// one of these, so with all of them varied the working set is as large as
    /// one conversion can make it.
    fn max_varied_options() -> Options {
        Options {
            default_link_dict: String::new(),
            bullets: "a_b_".to_string(),
            bullets_ordered: "z".to_string(),
            bold_delimiter: "@".to_string(),
            italic_delimiter: "\u{a3}".to_string(),
            underline_delimiter: "~".to_string(),
            hrule_min: 7,
            preformat_start_marker: "START".to_string(),
            preformat_end_marker: "STOP".to_string(),
            custom_heading_regexp: vec!["^H1".to_string(), "^H2".to_string()],
            ..Options::default()
        }
    }

    /// A document that reaches every construct that compiles a pattern: lists,
    /// definition lists, inline delimiters, tables, hrules, pre chunks, caps.
    fn maximal_document(sections: usize) -> String {
        let mut text = String::new();
        for i in 0..sections {
            text.push_str(&format!("--- section {i} ---\n"));
            text.push_str("H1 heading\nH2 heading\n");
            text.push_str("1. ordered\n* bullet\na_b_ x\nTerm: definition\n");
            text.push_str("@b@ \u{a3}i\u{a3} ~u~ #h# ^s^ |v| **w** <<n>>\n");
            text.push_str("-------\n");
            text.push_str("<pre>\npre line\n</pre>\n\n");
            text.push_str("START\npre start\nSTOP\n\n");
        }
        text
    }

    /// Part 1: the cache stays inside its cap. The only unbounded source is the
    /// user-supplied `custom_heading_regexp` list; this is insurance against a
    /// user option, not a fix for an attack on untrusted input.
    #[test]
    fn re_cache_bounded_by_a_measured_maximum() {
        let mut c = Converter::new(Options {
            default_link_dict: String::new(),
            // 400 patterns: comfortably past the cap, so the limit is reached
            // rather than approached.
            custom_heading_regexp: (0..400).map(|i| format!("^NOMATCH{i}")).collect(),
            ..Options::default()
        });
        // A line matching none of them, so all 400 are actually tried.
        c.convert_text("NOMATCH399 heading\n\nplain paragraph\n");

        assert!(
            c.re_cache.len() <= MAX,
            "the pattern cache grew to {} entries, past its cap of {MAX}",
            c.re_cache.len()
        );
    }

    /// Part 2: the cap is above the real working set, so a normal conversion
    /// never hits it. 19 is measured: what this document compiles with defaults
    /// and with every pattern-varying option set to a distinct value at once.
    #[test]
    fn the_cap_is_above_the_measured_working_set() {
        for opts in [
            Options {
                default_link_dict: String::new(),
                ..Options::default()
            },
            max_varied_options(),
        ] {
            let mut c = Converter::new(opts);
            c.convert_text(&maximal_document(30));
            let n = c.re_cache.len();
            assert!(
                n < MAX,
                "a normal conversion compiled {n} patterns, at or past the cap of \
                 {MAX}: the cap would be clearing on ordinary input"
            );
        }
    }

    /// Part 3: bounding the cache cannot change the output. The same document is
    /// converted with all patterns fitting and with 400 heading patterns forcing
    /// several clears; the outputs must be identical.
    #[test]
    fn clearing_the_cache_does_not_change_the_output() {
        let text = maximal_document(30);

        let mut small = Converter::new(Options {
            default_link_dict: String::new(),
            ..Options::default()
        });
        let expected = small.convert_text(&text);

        let mut big = Converter::new(Options {
            default_link_dict: String::new(),
            // 400 patterns, none of which match, so the cache is filled and
            // cleared repeatedly while the document is converted.
            custom_heading_regexp: (0..400).map(|i| format!("^NOMATCH{i}")).collect(),
            ..Options::default()
        });
        assert!(
            big.re_cache_len_for_test() < MAX,
            "precondition: the cache starts empty, so the clears happen during \
             the conversion rather than before it"
        );
        let got = big.convert_text(&text);

        assert_eq!(
            got, expected,
            "bounding the pattern cache changed the output"
        );
    }

    impl Converter {
        fn re_cache_len_for_test(&self) -> usize {
            self.re_cache.len()
        }
    }
}

/// Regex safety: a *compilable* pattern that backtracks catastrophically must
/// not be able to take a conversion down.
///
/// `fancy-regex` runs a pattern on a backtracking VM only when it uses a
/// look-around, a back-reference or similar; that VM reports an exhausted step
/// budget as `Err`, never as a match. Every place the engine consumes a
/// user-supplied pattern must therefore treat that `Err` as "no match" rather
/// than unwrapping it (an unwrap is a panic, i.e. exit 101). `(a+)+$` is the
/// classic exponential pattern and `a…a!` is the input that forces the full
/// search, so if a path unwrapped a runtime error these tests panic; if the
/// budget were removed they would hang instead of failing fast.
#[cfg(test)]
mod regex_safety_tests {
    use super::*;
    use crate::options::Options;

    /// A line that makes `(a+)+$` explore its search tree without matching.
    fn redos_line() -> String {
        format!("{}!\n", "a".repeat(2000))
    }

    #[test]
    fn a_catastrophic_heading_pattern_is_treated_as_no_match() {
        // Precondition: this input really does drive the engine to its budget,
        // so a regression that skipped the pattern would not pass vacuously.
        let re = crate::links::compile_pattern(r"(a+)+$", false);
        assert!(
            re.is_match(&redos_line()).is_err(),
            "the input should exhaust the backtrack budget"
        );

        let mut c = Converter::new(Options {
            custom_heading_regexp: vec![r"(a+)+$".to_string()],
            default_link_dict: String::new(),
            ..Options::default()
        });
        // Returning at all is the assertion: the line is not a heading, so the
        // pattern is tried and rejected rather than skipped by the prefilter.
        let html = c.convert_text(&redos_line());
        assert!(!html.is_empty());
    }

    #[test]
    fn a_catastrophic_preformat_marker_is_treated_as_no_match() {
        let mut c = Converter::new(Options {
            use_preformat_marker: true,
            preformat_start_marker: r"(a+)+$".to_string(),
            preformat_end_marker: r"b(b*)+$".to_string(),
            default_link_dict: String::new(),
            ..Options::default()
        });
        let html = c.convert_text(&redos_line());
        assert!(!html.is_empty());
    }
}

/// The streaming path must be byte-for-byte the buffered path for any
/// UTF-8 input, and must refuse input it cannot decode identically.
#[cfg(test)]
mod stream_tests {
    use super::*;
    use crate::options::Options;
    use std::io;

    fn opts() -> Options {
        Options {
            default_link_dict: String::new(),
            ..Options::default()
        }
    }

    fn buffered(options: &Options, text: &str) -> String {
        Converter::new(options.clone()).convert_text(text)
    }

    fn streamed(options: &Options, text: &str) -> String {
        let mut out = Vec::new();
        Converter::new(options.clone())
            .convert_stream(text.as_bytes(), &mut out)
            .expect("streaming a UTF-8 string should not fail");
        String::from_utf8(out).unwrap()
    }

    /// Inputs chosen for the record splitter's edges: blank/no blank, leading and
    /// repeated blanks, no trailing newline, CRLF separators, and paragraphs that
    /// drive the list/pre/table state machines.
    const BATTERY: &[&str] = &[
        "plain\n\nsecond para\n",
        "# heading\n\nbody *ital* and #bold#\n",
        "line1\nline2\n\n- a\n- b\n\n    pre\n\nend\n",
        "no trailing newline",
        "\n\nleading blanks\n\n\n\ndouble blank\n",
        "one\n\n\n\ntwo\n",
        "setext\n=====\n\nunderlined\n---------\n",
        "a\r\nb\r\n\r\nc\r\n",
        "ALIGN table\nx | y\n\nplain\n",
        "1. one\n2. two\n\n\n    code after list\n",
    ];

    #[test]
    fn stream_matches_buffered_on_every_battery_case() {
        let options = opts();
        for text in BATTERY {
            assert_eq!(
                streamed(&options, text),
                buffered(&options, text),
                "stream and buffered differ on {text:?}"
            );
        }
    }

    #[test]
    fn stream_matches_buffered_under_non_default_options() {
        let mut make_tables = opts();
        make_tables.make_tables = true;
        let mut xhtml = opts();
        xhtml.xhtml = true;
        let mut no_anchors = opts();
        no_anchors.make_anchors = false;
        let mut lower = opts();
        lower.lower_case_tags = true;
        lower.html5 = true;
        let mut extract = opts();
        extract.extract = true;
        let mut hrule = opts();
        hrule.hrule_min = 2;

        for options in [make_tables, xhtml, no_anchors, lower, extract, hrule] {
            for text in BATTERY {
                assert_eq!(
                    streamed(&options, text),
                    buffered(&options, text),
                    "stream and buffered differ under modified options on {text:?}"
                );
            }
        }
    }

    /// A document large enough to cross the record-by-record state machine many
    /// times.
    #[test]
    fn stream_matches_buffered_on_a_long_document() {
        let mut text = String::new();
        for i in 0..200 {
            text.push_str(&format!("# Section {i}\n\n"));
            text.push_str("Some *italic* and #bold# text.\n\n");
            text.push_str("- one\n- two\n\n    indented\n\n");
        }
        let options = opts();
        assert_eq!(streamed(&options, &text), buffered(&options, &text));
    }

    #[test]
    fn stream_rejects_invalid_utf8_without_writing_output() {
        let mut out = Vec::new();
        let err = Converter::new(opts())
            .convert_stream(&b"ok line\n\xff\xfe bad\n"[..], &mut out)
            .expect_err("invalid UTF-8 must be refused");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(out.is_empty(), "nothing should be written before the error");
    }

    #[test]
    fn stream_rejects_utf16_even_though_ascii_utf16_is_valid_utf8() {
        // 'a','b' in UTF-16LE: the NULs are valid UTF-8, so only the structural
        // check catches this.
        let mut out = Vec::new();
        let err = Converter::new(opts())
            .convert_stream(&[0xFF, 0xFE, b'a', 0, b'b', 0, b'\n', 0][..], &mut out)
            .expect_err("UTF-16 input must be refused");
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(out.is_empty());
    }
}
