# Offering — how textrill is offered, and against what

Written 2026-10-06. Status: **Authoritative**, checked by `make proof`
(`textrill/tests/positioning.py`), which re-measures every number in §3 and
fails if one drifts.

`docs/LANDSCAPE.md` answers *where the tool sits*. This document answers the
question after it: given that position, what exactly do we offer, to whom, in
what order, and what do we have to build first for the offer to be true. It
adds no engine work of its own — every engineering item it names is already in
`docs/PLAN.md` — it sequences them behind the claim.

---

## 1. The configuration we actually have

Measured 2026-10-06, not described. This is the inventory the offer rests on.

| Surface | State |
|---|---|
| CLI | 68 options (`textrill --help`), long spellings plus the reference's short aliases, `@file` groups |
| Option files | `@file` < `~/.textrillrc` < `./.textrillrc` < command line; legacy `.txt2htmlrc`/`.txt2html.dict` read only when the textrill name is absent in the same directory |
| Link dictionary | `--default_link_dict`, `links_dictionaries`, `.textrill.dict` |
| Templates | `--body_template` (body wrap), `--document_template` (whole document), `--template_library` (five shipped templates, embedded) and `--var name=value` filling `{{textrill:var:name}}`; 7 namespaced slots; an unknown `textrill:` slot is a hard error, every other `{{…}}` passes through untouched |
| Output modes | one file, `--extract` (body only), `--section`, `--chunk`, `--toc`, `--number_headings`, `--citations`, `--glossary` |
| Encoding | BOM → UTF-16 evidence → UTF-8 → CP1252, with explicit overrides for what cannot be detected |
| GUI | `textrill-gui-rs`, native `egui`; an options panel over the engine's own options; settings at `$XDG_CONFIG_HOME/textrill-gui/textrill.conf` (QSettings-compatible, hand-editable) |
| Binary | static `x86_64-unknown-linux-musl`, no runtime dependencies |
| Checks | `make verify`: fmt, clippy, 364 engine tests, 74 GUI tests, proptest, alloctest, 61 differential cases, 33 goldens, 16 000 fuzz cases |
| Packaging | Flatpak manifest drafted, deliberately unbuildable: `cargo-sources.json` needs the generate-vs-vendor decision (`docs/PACKAGING.md`) |
| Docs | the register in `DOCS.md`, `CAPABILITIES.md` (measured), `LANDSCAPE.md` (measured), `PLAN.md` (what is next) |

`--var` shipped (S3) and a five-template library shipped embedded in the
binary (S4); the remaining rows of §5 are what is still missing.

## 2. The claim

**textrill reads documents that are not written in a markup language, works out
what the layout implies, and writes HTML the user's own template governs.**

That is the offer, and its boundary is the interesting part:

- pandoc accepts 43 input formats, and `t2t` — txt2tags — is one of them. The
  nearest sibling to the original work is readable by pandoc. What pandoc does
  not accept is *no* format at all: `pandoc -f plain` is rejected outright, and
  forcing markdown over unmarked prose destroys the line structure that was the
  only structure there was (§3, P9–P15).
- So the ground we hold is not "text conversion" in general. It is *unmarked
  text*: notes, drafts, transcripts, changelogs, meeting minutes, chapter files
  saved without a markup language. A user who already has markdown has pandoc
  and should use it.
- We return HTML only, through templates the user controls. That is a smaller
  product than pandoc's and it is on purpose — see §7.
- And we **compose** with pandoc rather than rival it. HTML is pandoc's own
  reader, so textrill is the front half of a pipeline and pandoc the back half:
  it reads what we read and writes what we do not — `.epub`, `.docx`, `.pdf`,
  `.man`, markdown, all 64 of its output formats. What makes this more than a
  theoretical connection is that the structure *we* inferred survives the hand
  off: 113 bold runs and all 34 line breaks reach the `.docx`, and a heading
  nobody wrote comes back out as `**…**` in the markdown (§3, P17–P20).

### What we must not say

These are claims the measurements do not support, and saying them would make
§3 a marketing table rather than evidence:

1. *That pandoc is bad.* It is very good at its job, which is a different job.
2. *That we replace pandoc, or compete with it for formats.* One output
   format, and it is HTML. We are the reader it does not have, not an
   alternative to its writers (§2).
3. *That inference is certain.* It is a heuristic over layout, and every
   decline keeps the text (contract clause 3, `CAPABILITIES.md` §6). The
   reported counts from `make examples` are part of the output, not a debug
   mode — and `--report` puts them on stderr for any script that wants them
   (S6).
4. *That the Perl reference is our specification.* It is an oracle for one
   tier of tests. Two of the reference's wrongnesses are already recorded
   (encoding, and no scheme policy).

## 3. The proof

Everything in this table is re-measured by `make proof` and pinned in
`textrill/tests/positioning.py`. The commands are here so a number can be
checked by hand:

```sh
textrill --help | grep -cE '^    '                       # 68 options
pandoc --list-input-formats                              # no plain, 43 total
pandoc -f plain -t html examples/homer.txt               # rejected
textrill --infile examples/homer.txt --outfile -         # 38 477 B, 64 p, 3 030 max
pandoc -f markdown -t html examples/homer.txt            # 38 132 B, 62 p, 3 665 max

# the conjunction: our HTML into pandoc's writers
textrill --title "The Odyssey" --infile examples/homer.txt --outfile - \
  | pandoc -f html -t epub -o /tmp/homer.epub            # valid epub
textrill --title "The Odyssey" --infile examples/homer.txt --outfile - \
  | pandoc -f html -t docx -o /tmp/homer.docx            # 113 bold, 34 breaks
```

| # | Claim | Measured |
|---|---|---|
| P0 | pandoc measured against | 3.1.3 (`competition-files/` holds the 3.12 source) |
| P1 | pandoc has no plain-text reader | `plain` absent from `--list-input-formats`; 43 readers |
| P2 | `pandoc -f plain` | rejected, non-zero exit |
| P3 | the 3.12 source registers no plain reader | no `"plain"` entry in `Readers.hs` |
| P4–P8 | textrill on `examples/homer.txt` | 39 `<strong>`, 34 `<br>`, 0 headings, 64 paragraphs, largest 3 030 chars, 38 477 B |
| P9–P12 | pandoc, forced to markdown, same file | 0 `<strong>`, 0 `<br>`, 62 paragraphs, largest 3 665 chars, 38 132 B |
| P13–P15 | mechanism, small input | textrill keeps indented lines apart; pandoc joins them into one paragraph |
| P16 | our own surface | 68 options |
| P17–P18 | our HTML into `pandoc -t epub` | exits 0; the result carries `application/epub+zip` |
| P19 | our HTML into `pandoc -t docx` | 113 bold runs, 34 line breaks — the inference reached the far format |
| P20 | our HTML into `pandoc -t markdown` | `**PREFACE TO FIRST EDITION**` — a heading nobody wrote survives |

P9–P12 are the sentence to quote: pandoc forced over unmarked text recovers
**no emphasis and no line breaks at all**, and merges the title block, contents
list and first chapter headings into one 3 665-character paragraph, because
markdown's soft-wrap rule assumes the author never meant a line break. textrill
recovers 39 and 34 from the same bytes and keeps the paragraph boundaries.

P17–P20 are the other half: the same 39 and 34, handed to pandoc, are still 113
bold runs and 34 breaks in a `.docx` and still `**…**` in markdown. Inference
that dies at the format boundary would be worth nothing to anyone, so the
pipeline claim is gated rather than asserted.

`make proof` requires pandoc and is deliberately **not** part of `make verify`
— verify must stay runnable on a machine with only Rust and perl. Drift in this
table is the failure mode `legacy-archive/TOOL-SURVEY.md` had: a matrix that
went false while still looking authoritative. The gate is how this document
avoids the same end.

## 4. The offer, by who is asked

| Reader | What we say | What we give them |
|---|---|---|
| Someone with a `.txt` and no markup | "No markup required, and no markup language assumed." | `textrill notes.txt -o notes.html`, or the GUI |
| Someone who already has markdown/pandoc | "You already have the better tool for that file." | Nothing — and that answer is part of the credibility |
| Someone who needs `.epub`, `.docx`, `.pdf`, `.man`, markdown | "We read the file; pandoc writes the format." | `textrill notes.txt --outfile - \| pandoc -f html -t epub` — and the structure we inferred is still in it (§3, P17–P20) |
| A writer with a draft | structure recovered and *counted*, so they can see what was inferred | `--toc`, `--number_headings`, `--section`, a shipped article/book template (§5) |
| Someone packaging or scripting | a single static binary, no runtime, no network, no telemetry | `make musl`, CI already runs the corpus against that binary |
| A reviewer asking "what does it actually do" | `make examples` prints the counts; `docs/CAPABILITIES.md` reproduces each weak point | the ten-clause output contract |

The two things that make this offer credible rather than asserted: the counts
are printed by the tool itself, and the weaknesses are published in the same
document as the strengths.

## 5. What has to be built for the offer to be true

Ordered by what a first-time user meets first. Each row is an existing plan
item; this document's contribution is the order and the reason.

| Order | Item | Why here |
|---|---|---|
| 1 | the proof (§3) | Positioning claims rot silently; this is now gated. |
| 2 | `PLAN.md` Phase 4.1 — `--var name=value` | **Built (S3).** The single highest-value addition is delivered: a fixed frame is now parameterisable, `{{textrill:var:name}}` filled verbatim and strict on undeclared names. |
| 3 | `PLAN.md` Phase 4.2 — a shipped template library | **Built (S4).** Five templates ship embedded in the binary and as files under `textrill/templates/` (`article`, `book`, `manpage`, `slide`, `bare`), selected with `--template_library NAME`. All use only the fixed slots, so each works with zero required arguments and produces no silent-empty frames; someone with no template now has actual ones to start from. |
| 4 | `PLAN.md` Phase 5.0 — `--report`, the counts on stderr | **Built (S6).** `--report` prints the five inference counts on stderr once the output is written — bytes, headings, paragraphs, capitalised runs, line breaks — and `make examples` reads them from that line instead of re-computing them, so a user whose `--toc` came out empty can now see what the engine saw. |
| 5 | `PLAN.md` Phase 5 — headings in documents that have none | `homer.txt` yields no headings at all: 39 capitalised runs become `<strong>`, `--toc` lists nothing, `--chunk` writes one 38 477-byte file. This is the capability claim a reader will test first ("it found my chapter headings, or it did not"), and the corpus-wide measurement is done (`make measure`, S8): every candidate heading rule is at or near 100% false positives on the no-structure documents — Blake and Calligrammes are all hits, Homer is the two signatures, and Gelbenhügel's `---` lines read as 16 headings its author never wrote. Research before behaviour: the Phase 5 change stays a proposal. |
| 6 | `PLAN.md` Phase 6 — packaging | A static binary is already buildable; an installable app is not. The GitHub owner is resolved (`youtubescholar`): both `repository` fields and the app-id `io.github.youtubescholar.Textrill` follow from it. The remaining blocker is `docs/PACKAGING.md`'s generate-vs-vendor decision. |
| 7 | `PLAN.md` Phase 7 — grow `examples/` | **Built (S7).** One document is not evidence of trustworthiness on *your* document. The corpus is now eight: Greek Septuagint (Swete), Hebrew/Aramaic Talmud (Vilna text), two Classical Chinese works, William Blake, Apollinaire's *Calligrammes*, a CC0 tabletop-RPG setting, and the Odyssey. Every file's source, edition and CC0/PD confirmation are recorded in `examples/README.md`, and `reporttest` pins each one's recovered counts. |
| 8 | Phase 4.3 — `{{textrill:if:…}}` | One template serving documents with and without a TOC. Valuable, and after the library exists rather than before it. |

A reader meets the work in this order; it is not a second authority. The build
order — which additionally schedules Phase 2.3–2.4 and Phase 3, and which runs
this row 6 (corpus growth) *before* Phase 5's measurement because that
measurement is over `examples/` — is `PLAN.md`'s Sequencing, one step per item
with its verification check, and that governs.

### 5.1 The `homer.txt` path, worked

The shape of real use, and the reason items 2–4 are ordered as they are: a user
writes their own page, puts `{{textrill:content}}` in it, and feeds the tool
raw text. Measured 2026-10-06:

**Works today.** `--document_template page.html --title "The Odyssey" --infile
examples/homer.txt` fills both `{{textrill:title}}` and `{{textrill:content}}`
with the converted body. `--var byline=… --var date=… translation=…` now fills
any further `{{textrill:var:name}}` slots the same way. `--extract` gives the
body alone for dropping into an existing page. `-H <regexp>` takes the user's
own heading convention, and `{{textrill:pager}}` renders the previous/next
links.

**Does not work yet, in the order a user meets it:**

1. *`--toc` emits zero items and `--chunk` emits one 38 477-byte file*, because
   no line in the document becomes a heading. The "nothing says so" half of this
   is closed: `--report` (S6) prints `headings=0` on stderr, and `make examples`
   reads the same line, so an empty table of contents is at least visible as a
   report of what was seen. The heading *behaviour* still waits on the
   false-positive measurement of item 3.
2. *`-H` only fires on a line that starts a paragraph.* Measured: `-H 'PREFACE'`
   → 2 headings, `-H 'THE ODYSSEY'` → 1, `-H 'BOOK I'` → 0, because in the body
   `BOOK I` is the second line of a three-line title block and in the contents
   list it is one line of twenty-four consecutive ones. No regexp can pick out
   a line the rule will not consider. The help text does not say this, and it
   is the first thing a user will hit when they try to teach the engine their
   document's convention.
3. *Phase 5's candidate rule would not fire here either.* "A short all-caps
   line alone between blank lines" matches exactly 2 lines of this document —
   `S. BUTLER.` and `HENRY FESTING JONES.`, both signatures — and none of the
   three real section starts, because the title block is three consecutive
   caps lines and the contents list is 24. The plan requires a false-positive
   measurement before any behaviour change; this is the first true-negative
   measurement of the candidate rule itself, and it belongs in the same pass.

So the honest answer for a `homer.txt` today is: the template path works,
`--report` names what was recovered (S6), the TOC and chunking do not, `-H`
covers documents whose section titles start a paragraph, and everything else
waits on the measurement the plan already requires. The items above are what
close it.

Nothing else is missing for the offer to be honest. Multi-format output is
excluded by `PLAN.md` § Not doing, and §7 says why that is a position rather
than a gap.

## 6. First public release

`0.1.0` is the first release of the fork and is not published. What publishing
it needs, in order:

1. ~~**The GitHub owner.**~~ **Resolved** 2026-10-08: the repo is
   https://github.com/youtubescholar/textrill. Both `repository` fields are set
   and the Flatpak app-id is `io.github.youtubescholar.Textrill`. What remains
   to publish, in order:
2. **The README's opening, rewritten against §2** — the current opening still
   spends its second paragraph on upstream provenance, which is the right fact
   in the wrong place for a reader deciding whether to use the tool.
3. **A published version of §3**, so the comparison is checkable rather than
   claimed: the table, the commands, and `make proof`.
4. **Musl binary artifacts** attached to the release; CI already builds and
   tests that binary.
5. **CHANGELOG** starting at 0.1.0. The git history is detailed enough to
   summarise, not to replace.
6. Flatpak, only if a desktop store presence is wanted; the manifest's
   remaining blocker (generate-vs-vendor) is in `docs/PACKAGING.md`.

## 7. What pandoc does better — publish this

None of this argues against installing pandoc. It is the back half of our own
pipeline (§2), and the honest reading of the two tables below is "install both":
we are the reader it does not have, it is the writer we do not have.

Positioning that only lists strengths is the thing `DOCS.md` was written to
stop. The honest list:

| pandoc | textrill |
|---|---|
| 64 output formats: docx, epub, pdf, LaTeX, man, … | HTML only |
| a real template language: `-V` variables, `$if`, `$for`, partials | 7 slots + `--var` parameters, no loops or conditionals (4.2–4.3 closes part of this) |
| bibliography and citation processing (CSL, biblatex) | `--citations` collects markers into endnotes; no bibliography engine |
| filters (Lua, JSON) and a documented AST | no AST surface at all |
| dozens of contributors, a decade of releases | one maintainer |

And the reverse table, which §3 backs: pandoc cannot read unmarked text, and
recovering structure from layout is not a feature it plans to add.

## 8. Risks

- **Claim rot.** The §3 table is the same species of document as the archived
  feature matrix that went false. Mitigated by `make proof`; not mitigated for
  any claim *not* in the table, which is why §2's "must not say" list exists.
- **pandoc gains a plain-text reader.** Then P1–P3 fail and the claim has to be
  rewritten from a stronger position, with measurements. That is the correct
  outcome, not a problem with the gate.
- **Inference is wrong on someone's real document** (Phase 5's false-positive
  question). The mitigation is the measurement the plan already requires, and
  publishing the counts so a wrong inference is visible to the user.
- **Parity framing regressing.** Two documents still explain the Perl
  relationship at length. That is legitimate provenance; it becomes a problem
  if it reappears as the reason the tool exists.

## 9. Gates

| Gate | Command | Fails on |
|---|---|---|
| this document's numbers | `make proof` (needs pandoc) | any drift in §3 |
| behaviour | `make verify` | fmt, clippy, tests, corpus, fuzz |

`make proof` was broken on purpose before it was believed: pinning an option
surface of 65 against a measured 64 exits 1 and names the claim; pinning the
docx structure at (113, 34) against a hand-edited (112, 34) does the same for
P19; and with pandoc absent it exits 2 with a message naming pandoc rather than
reporting success.
