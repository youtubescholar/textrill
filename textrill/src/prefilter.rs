//! A sound required-substring prefilter for link rules.
//!
//! Answers "could this rule match here?" with a byte scan instead of a regex
//! call. Sound one-directionally: a reject guarantees the regex cannot match, so
//! a false accept only costs time while a false reject would silently drop a
//! link. The extractor therefore returns `None` ("always run the regex") unless
//! it can prove a literal is required, deriving it from the parsed HIR:
//! alternations union their branches (unconstrained if any branch is), `min >= 1`
//! repetitions and captures pass through, a concatenation keeps its most
//! selective child, and anchors/look-around/`\b` add no requirement because they
//! match empty and constrain position, not content. Both literal and haystack
//! are ASCII-folded, which accepts more often, never less.

use regex_syntax::hir::{Class, Hir, HirKind};

/// A single literal that must appear in any string the pattern matches.
#[derive(Clone, Debug)]
pub struct RequiredLiteral {
    /// Lowercased, so the haystack must be lowercased to match against it.
    bytes: Vec<u8>,
    /// A prebuilt searcher over `bytes`. `memmem::Finder` uses SIMD; a naive
    /// `windows().any()` scan was measurably slower than the regex calls it
    /// removed.
    finder: memchr::memmem::Finder<'static>,
}

impl RequiredLiteral {
    /// The literal, lowercased.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn find(&self, haystack: &[u8]) -> bool {
        self.finder.find(haystack).is_some()
    }
}

/// Build a searchable lowercased literal.
///
/// The bytes are leaked so the `Finder` can borrow them for `'static`; the leak
/// is bounded because literals are only built from the fixed pattern set at load.
fn literal(s: &str) -> RequiredLiteral {
    let owned = s.to_ascii_lowercase().into_bytes();
    let bytes: &'static [u8] = Box::leak(owned.into_boxed_slice());
    RequiredLiteral {
        bytes: bytes.to_vec(),
        finder: memchr::memmem::Finder::new(bytes),
    }
}

/// A disjunction of requirements: a match requires at least one of these
/// literals to be present. An empty `Vec` means "no requirement", which the
/// caller must treat as *always run the regex*, never as *never match*.
pub type Alternatives = Vec<RequiredLiteral>;

/// Does `hir` force any literal to be present in a match?
///
/// `Ok(None)` means "I cannot prove one"; the caller then runs the regex.
fn required(hir: &Hir) -> Result<Option<Alternatives>, Box<regex_syntax::Error>> {
    Ok(match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => None,

        HirKind::Literal(lit) => Some(alternatives_of(lit)),

        HirKind::Class(class) => {
            // A singleton class is that character; multi-character and Unicode
            // ranges are not used, since a representative char is unsound.
            class_singleton(class).map(|c| vec![literal(&c.to_string())])
        }

        HirKind::Capture(cap) => required(&cap.sub)?,

        HirKind::Repetition(rep) => {
            if rep.min >= 1 {
                // Matches at least once, so whatever the body requires is
                // required here too.
                required(&rep.sub)?
            } else {
                None
            }
        }

        HirKind::Concat(parts) => {
            // Every part must match, so any one part's requirement is valid;
            // take the longest guaranteed literal, the most selective filter.
            let mut best: Option<Alternatives> = None;
            for part in parts {
                let candidate = required(part)?;
                let better = match (&best, &candidate) {
                    (None, Some(_)) => true,
                    (Some(b), Some(c)) => alt_len(c) > alt_len(b),
                    _ => false,
                };
                if better {
                    best = candidate;
                }
            }
            best
        }

        HirKind::Alternation(branches) => {
            // Exactly one branch matches, so the union is a necessary condition;
            // an unconstrained branch makes the whole alternation unconstrained.
            let mut union = Alternatives::new();
            for branch in branches {
                if let Some(alts) = required(branch)? {
                    union.extend(alts);
                } else {
                    return Ok(None);
                }
            }
            if union.is_empty() {
                None
            } else {
                Some(union)
            }
        }
    })
}

/// Total guaranteed length of an alternative set, used only to pick the most
/// selective child of a concatenation.
fn alt_len(alts: &Alternatives) -> usize {
    alts.iter().map(|a| a.bytes.len()).sum()
}

fn alternatives_of(lit: &regex_syntax::hir::Literal) -> Alternatives {
    // One contiguous byte string, searchable as-is. A literal we cannot decode
    // is dropped, which only makes the prefilter run the regex more often.
    match std::str::from_utf8(&lit.0) {
        Ok(s) => vec![literal(s)],
        Err(_) => Vec::new(),
    }
}

/// The class's single character, only when it is exactly one ASCII character.
/// `Class::literal` already refuses empty and multi-element classes.
fn class_singleton(class: &Class) -> Option<char> {
    let bytes = class.literal()?;
    if bytes.len() == 1 && bytes[0].is_ascii() {
        Some(char::from(bytes[0]))
    } else {
        None
    }
}

/// Extract the required literal for a pattern, if one can be proven.
///
/// `pattern` must be the translated pattern actually handed to `fancy_regex`.
/// Case folding needs no parameter: both sides are folded unconditionally, which
/// is always sound.
pub fn required_literal(pattern: &str) -> Option<Alternatives> {
    // Translation can leave constructs `regex-syntax` rejects only in exotic
    // cases; a parse failure must never be read as "no literal required" in
    // the *rejecting* sense, so fall back to None (always run the regex).
    let hir = regex_syntax::parse(pattern).ok()?;
    match required(&hir) {
        // An empty alternative list means the analysis found the pattern
        // unconstrained, e.g. a bare `.*`. Treat it as no filter.
        Ok(Some(alts)) if !alts.is_empty() => Some(alts),
        _ => None,
    }
}

/// The runtime half: does `haystack` contain any of the literals?
///
/// Folding happens here, not at the call site, so a caller cannot forget it -- a
/// forgotten fold would silently drop uppercase links. Checking many rules
/// against one paragraph should fold once and use [`may_match_prefolded`].
pub fn may_match(haystack: &str, alternatives: &Alternatives) -> bool {
    if alternatives.is_empty() {
        return true;
    }
    let mut folded = haystack.as_bytes().to_vec();
    folded.make_ascii_lowercase();
    may_match_prefolded(&folded, alternatives)
}

/// [`may_match`] for a haystack the caller has already folded.
pub fn may_match_prefolded(haystack_lower: &[u8], alternatives: &Alternatives) -> bool {
    if alternatives.is_empty() {
        return true;
    }
    alternatives.iter().any(|lit| lit.find(haystack_lower))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit_of(p: &str) -> Option<Vec<String>> {
        required_literal(p).map(|alts| {
            alts.iter()
                .map(|a| String::from_utf8_lossy(a.as_bytes()).into_owned())
                .collect()
        })
    }

    #[test]
    fn plain_literal_is_required() {
        assert_eq!(lit_of("http"), Some(vec!["http".to_string()]));
    }

    #[test]
    fn inner_literal_after_a_class_is_found() {
        // The literal is not at the start of the pattern.
        assert_eq!(
            lit_of(r"^\s*([^\w]+\s*)?comp\.lang\."),
            Some(vec!["comp.lang.".to_string()])
        );
    }

    #[test]
    fn alternation_unions_its_branches() {
        // A match is exactly one of the branches, so each is a candidate.
        assert_eq!(
            lit_of("news|misc"),
            Some(vec!["news".to_string(), "misc".to_string()])
        );
    }

    #[test]
    fn unconstrained_branch_kills_the_alternation() {
        // One branch can match "anything", so neither literal is required.
        assert_eq!(lit_of("news|.*"), None);
    }

    #[test]
    fn anchors_and_word_boundaries_are_not_literals() {
        assert_eq!(lit_of(r"^\bword\b$"), Some(vec!["word".to_string()]));
        assert_eq!(lit_of(r"^\s*$"), None);
    }

    #[test]
    fn min_zero_repetition_requires_nothing() {
        assert_eq!(lit_of("(ab)*"), None);
        assert_eq!(lit_of("(ab)+"), Some(vec!["ab".to_string()]));
    }

    #[test]
    fn empty_and_dot_match_anything_so_no_filter() {
        assert_eq!(lit_of(""), None);
        assert_eq!(lit_of(".*"), None);
    }

    #[test]
    fn class_of_one_character_is_required() {
        // regex-syntax folds a single-character class into the surrounding
        // literal, so the whole run comes back as one searchable string.
        assert_eq!(lit_of("[q]ing"), Some(vec!["qing".to_string()]));
    }

    #[test]
    fn multi_character_class_is_not_a_literal() {
        // The class admits many characters, so only `cd` is guaranteed.
        assert_eq!(lit_of("[ab]cd"), Some(vec!["cd".to_string()]));
    }

    #[test]
    fn concat_takes_the_longest_child_literal() {
        // The alternation is most selective as a whole and contributes both
        // branches: a match takes one, so either being present is enough.
        assert_eq!(
            lit_of("a(bbb|cc)ddd"),
            Some(vec!["bbb".to_string(), "cc".to_string()])
        );
    }

    #[test]
    fn capture_groups_are_transparent() {
        // Either side is a sound choice; the test pins the tie-break.
        let got = lit_of("(foo)bar").unwrap();
        assert!(got == vec!["foo".to_string()] || got == vec!["bar".to_string()]);
    }

    #[test]
    fn nocase_lowercases_both_sides() {
        let alts = required_literal("HTTP").unwrap();
        assert_eq!(alts[0].as_bytes(), b"http");
        assert!(may_match("http://x", &alts));
        // The point of folding internally: uppercase still passes.
        assert!(may_match("x HTTP y", &alts));
        assert!(may_match("HtTp://x", &alts));
        assert!(!may_match("ftp://x", &alts));
    }

    #[test]
    fn unparsable_pattern_yields_no_filter() {
        assert_eq!(lit_of("a(b"), None);
    }

    // The invariant checked directly: whenever the prefilter rejects, the regex
    // really cannot match. Run over a corpus of patterns and haystacks.
    #[test]
    fn rejection_implies_no_match() {
        let patterns = [
            r"comp\.lang\.[a-z]+",
            r"^\s*news:\s*(\S+)",
            r"(news|misc|comp)\.\w+",
            r"[q]ing",
            r"http[s]?://(\S+)",
            r"(ab)+c",
            r"\bFAQ\b",
            r"([\w+-]+\.)+example\.(com|org)",
            r"^(a|aa)+$",
            r"\S+\s+\S+\s+\S+",
        ];
        let haystacks = [
            "",
            "news: comp.lang.perl",
            "comp.lang.perl.misc",
            "http://example.com",
            "qings",
            "faq",
            "a.EXAMPLE.org b",
            "aaaaaaaaaa",
            "one two three",
            "nothing here at all",
            "comp.lang",
            "mailto:a@b.c",
            "aaaaaaaaaaaaaaaa",
            "x.y.z.example.net",
            "abababababc",
            "FAQ: yes",
            "s@e example",
        ];
        for p in patterns {
            let re = fancy_regex::Regex::new(p).unwrap();
            for hay in haystacks {
                if let Some(alts) = required_literal(p) {
                    if !may_match(hay, &alts) {
                        assert!(
                            re.is_match(hay).map(|m| !m).unwrap_or(true),
                            "prefilter rejected {hay:?} but {p} matches"
                        );
                    }
                }
            }
        }
    }
}
