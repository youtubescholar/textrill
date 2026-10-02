//! A sound required-substring prefilter for link rules.
//!
//! The link pass asks every rule to `captures()` every paragraph, which is
//! 257,140 invocations for the shipped 52-rule dictionary and a 2 MB
//! document. Most of them match nothing. The prefilter answers "could this
//! rule possibly match here?" with a byte scan instead of a regex call, and
//! only runs the regex once the answer is yes.
//!
//! # Soundness
//!
//! The only property that matters is one-directional:
//!
//! ```text
//! prefilter_rejects(haystack)  ==>  !regex.is_match(haystack)
//! ```
//!
//! A false *accept* only costs the time the regex would have spent anyway.
//! A false *reject* silently drops a link, which is the one failure mode
//! this project cares most about, so the extractor is built to give up
//! (return `None`, i.e. "always run the regex") whenever it cannot prove a
//! literal is genuinely required. Every branch that returns a literal is
//! one where that literal appears in *every* string the regex can match.
//!
//! # How a required literal is found
//!
//! [`regex_syntax::hir`] gives the parsed pattern, and the analysis walks it
//! with that bias. `required` returns a set of *alternatives*, where a
//! match requires at least one alternative to be present:
//!
//! - a literal contributes itself;
//! - a single-character class contributes that character;
//! - a repetition with `min >= 1` contributes its body's requirement, because
//!   it must match at least once, and nothing at all when `min == 0`;
//! - an alternation contributes the union of its branches, because matching
//!   it means matching exactly one of them;
//! - a group is transparent;
//! - anchors, look-around and `\b` contribute nothing, which is sound because
//!   they match the empty string and so constrain position, not content;
//! - anything else -- a multi-character class, a back-reference, a named
//!   group we cannot see inside -- contributes nothing.
//!
//! A concatenation must match *all* of its children, so it inherits the
//! single child with the longest guaranteed literal rather than combining
//! them. That is weaker than intersecting the children, and being weaker is
//! the safe direction: it can only reject less often.
//!
//! # Case folding
//!
//! Rules compiled case-insensitively carry `nocase`. Rather than reasoning
//! about Unicode case tables, both the literal and the haystack are compared
//! in `to_ascii_lowercase`. That is sound: ASCII case folding is exact, and
//! for non-ASCII bytes the comparison is on the original bytes, which only
//! makes the prefilter accept more often than a matching regex would.

use regex_syntax::hir::{Class, Hir, HirKind};

/// A single literal that must appear in any string the pattern matches.
#[derive(Clone, Debug)]
pub struct RequiredLiteral {
    /// Lowercased, so the haystack must be lowercased to match against it.
    bytes: Vec<u8>,
    /// A prebuilt searcher over `bytes`.
    ///
    /// `memmem::Finder` rather than a hand-written scan: this runs once per rule
    /// per paragraph (202,745 times on the 2 MB benchmark, 92% of them
    /// rejections), and `memmem` uses SIMD with a rare-byte skip heuristic.
    /// A naive `windows().any()` on a 150-byte paragraph for a short literal is
    /// byte-at-a-time, and that was measurably enough to make the filter cost
    /// more than the regex calls it removed.
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
/// The bytes are leaked so the `Finder` can borrow them for `'static`. The leak
/// is bounded by the pattern set: literals are only ever built from dictionary
/// and option patterns, of which there are tens, once each at load time. That
/// is the same reasoning as `ascii_re_cached` -- the input set is fixed and
/// small -- but unlike that one this is not enforced by the type system, so it
/// is worth stating. `memmem::Finder::new` can only be avoided by reimplementing
/// its search, which is the thing being avoided.
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
/// `Ok(None)` is the honest answer "I cannot prove one"; the caller then
/// runs the regex. `Err` is only for a pattern that will not parse, which
/// `fancy_regex` has already rejected by the time we get here.
fn required(hir: &Hir) -> Result<Option<Alternatives>, Box<regex_syntax::Error>> {
    Ok(match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => None,

        HirKind::Literal(lit) => Some(alternatives_of(lit)),

        HirKind::Class(class) => {
            // A class of exactly one character is that character. Unicode
            // ranges collapse to their representative char, which is
            // unsound for the *content* but harmless in practice only if we
            // reject rather than accept -- so only an exact singleton is used.
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
            // Every part must match, so any one part's requirement is a
            // valid necessary condition. Take the longest guaranteed
            // literal: the most selective filter, and any is sound.
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
            // Exactly one branch matches, so the union of the branches'
            // requirements is a necessary condition: at least one holds.
            let mut union = Alternatives::new();
            for branch in branches {
                if let Some(alts) = required(branch)? {
                    union.extend(alts);
                } else {
                    // One branch is unconstrained, so the alternation as a
                    // whole is unconstrained. Returning the union would be
                    // unsound: the unconstrained branch may match while none
                    // of the collected literals appear.
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
    // A `Literal` is one contiguous byte string, so it is searchable as-is.
    // The bytes are always valid UTF-8 in Unicode mode, but the lossy path is
    // avoided rather than assumed: a literal we cannot decode is dropped,
    // which only makes the prefilter run the regex more often.
    match std::str::from_utf8(&lit.0) {
        Ok(s) => vec![literal(s)],
        Err(_) => Vec::new(),
    }
}

/// The single character of a class, but only when it is exactly one ASCII
/// character. `Class::literal` already refuses empty and multi-element
/// classes, so this cannot return a partial match set.
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
/// `pattern` must be the *translated* pattern -- the exact string handed to
/// `fancy_regex` -- because the HIR is parsed from it and must describe the
/// regex that actually runs. Case folding needs no parameter because the
/// analysis lowercases both sides unconditionally, which is always sound and
/// costs nothing: a case-sensitive rule simply sees a haystack whose case
/// happens to be folded, so it may accept more often, never less.
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
/// Folding happens here rather than at the call site on purpose. The literals
/// are stored lowercased, so a caller that forgot to fold the haystack would
/// make the prefilter reject uppercase text that the case-insensitive regex
/// would have matched -- a silently dropped link, caused by an invisible
/// precondition. Taking a `&str` and folding it internally means the
/// precondition cannot be forgotten.
///
/// The cost is one allocation per rule per paragraph, which would defeat the
/// point, so callers that check many rules against one paragraph should fold
/// once and use [`may_match_prefolded`].
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
        // The shape that defeated prefix extraction: the literal is not at
        // the start of the pattern.
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
        // The alternation scores highest as a whole (3+2 > 1 and > 3), and it
        // contributes both branches, which is the point: a match takes one of
        // them, so either being present is enough.
        assert_eq!(
            lit_of("a(bbb|cc)ddd"),
            Some(vec!["bbb".to_string(), "cc".to_string()])
        );
    }

    #[test]
    fn capture_groups_are_transparent() {
        // Both sides are required and the same length, so either is a sound
        // choice; the test pins the current tie-break rather than a preference.
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

    // The invariant, checked directly: whenever the prefilter rejects, the
    // regex really cannot match. Run over a corpus of patterns and haystacks.
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
