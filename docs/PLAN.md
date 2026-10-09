# textrill — plan

Written 2026-10-06. Replaces `legacy-archive/REMEDIATION-PLAN.md`, which
answered a different question. That plan asked what the Perl original did that
this port did not, and its 65-option inventory existed to produce a list of
gaps. Producing that list is what kept the work pointed at mimicry, and it hid
the things that were actually wrong.

This plan is built from `docs/CAPABILITIES.md` (what the tool does, measured) and
`docs/LANDSCAPE.md` (who else does it). Every item below traces to a measurement
in one of those two documents — except Phase 8, which traces to the oracle
census in `docs/ORACLE-ARCHAEOLOGY.md`.

---

## What this is

textrill converts **plain text** to HTML. The input is not required to carry any
markup language; structure is inferred from layout, and the result is placed
into HTML through templates the user controls.

**Licensing is settled and not revisited.** GPL-3.0-or-later, with the Perl
original credited to Seth Golub, Kathryn Andersen and Joao Eriberto Mota Filho.
That text in `textrill/LICENSE` is correct and stays.

**The Perl reference stays available** at `ref/` as an oracle for the parity
tier of the test harness and as a historical artifact. It is no longer the
specification. Where the two disagree, the question is now "which is right",
answered per input class, and answered by us. Phase 8 retires it as a **gate**
once reference-free acceptance exists (S12); it remains an optional cross-check.

---

## Principles

1. **No markup required.** That is the product. Anything that makes the user
   mark the document up first is a different tool.
2. **Never drop text.** Declining to infer structure is fine. Dropping or
   silently reordering characters is not.
3. **Infer, then say so.** `make examples` reports what was recovered — and the
   CLI's own `--report` (S6) prints the same counts on stderr — so a regression
   in inference is a changed number rather than a shrug.
4. **The user's template is the user's.** No imposed page, no theme.
5. **Errors are errors.** Non-zero exit, nothing half-written.

---

## Phase 1 — Give the tool its own name

Nothing here changes behaviour. All of it was the port wearing the Perl module's
identity — embarrassing in every document produced, and wrong in the `generator`
metadata. **All four items are done**; the notes below record what each cost and
which gate holds it.

| # | item | why |
|---|---|---|
| 1.1 | ~~`generator` meta says `textrill`, not `HTML::TextToHTML v3.0`~~ **done** | `convert.rs` hard-coded the Perl module. Every file the tool writes claimed Perl made it. Attribution belongs in `LICENSE`, not in provenance. See "Item 1.1, resolved" below. |
| 1.2 | ~~Read `~/.textrillrc` and `./.textrillrc`; keep `.txt2htmlrc` working~~ **done** | Same precedence order, new preferred names. See "Items 1.2–1.4, resolved". |
| 1.3 | ~~Read `~/.textrill.dict` / `.textrill.dict`; keep the old names~~ **done** | Same. |
| 1.4 | ~~`--help` line 1 describes textrill~~ **done** | Was: *"A reimplementation of txt2html 3.0"*. |

Items 1.2–1.4 are compatibility-preserving, so the parity tier should stay green:
corpus 60/60 with 33/33 goldens after the change, as recorded below.

### Item 1.1, resolved

**textrill names itself.** The port produces these documents, and every one of
them said otherwise. Measured cost of the fix: 18 differential cases and 13
golden comparisons fail, which is the real number and not the 13 originally
estimated — the differential path was affected too, since a fresh Perl run
names itself as well.

The three options were normalize / suppress / keep. **Normalize** was chosen,
via `tests/corpus/normalize.py`: it canonicalises the one `meta name="generator"`
line to a sentinel on both sides before comparison. The alternatives were
rejected for recorded reasons:

- `NOGOLDEN[]` on the 13 stems suppresses the comparison entirely, so a real
  regression in them goes unseen. Five are list cases — the family holding two
  known inherited defects. Losing golden coverage there to save editing one
  string is a bad trade.
- Keeping the Perl string costs nothing and leaves the tool misattributing
  itself in every file it writes.

Normalising excludes the line from comparison, so it cannot on its own assert
the value. `tests/provenance.rs` is the other half: it requires the exact
expected string, fails if `TextToHTML` reappears in any form, pins both
tag-case branches and HTML5 mode, checks the version against `CARGO_PKG_VERSION`
so a Cargo.toml bump cannot leave a stale string, and runs once through the
spawned binary. The pair is a complete gate — the corpus proves content parity
with the reference, the test proves correct provenance.

Both halves were verified by breaking them on purpose, per § Standing rule:

| sabotage | expected | observed |
|---|---|---|
| `PROG` back to `HTML::TextToHTML` | provenance test red, corpus green | 5/5 failed, corpus 60/60 and 33/33 |
| every heading level `+1` (`h1`→`h2`) | corpus red | exit 1, 11 differential and 11 golden failures, `NORMALISED` unchanged at 67 |

The second row is the one that matters for `normalize.py`: a genuine content
regression 2 000 lines away from the generator still produced 11 golden
failures, so the normaliser is not a general one that swallows differences.

`NORMALISED: 67` is printed on every corpus run, including when it is zero. A
normalisation that quietly stopped applying would still leave the corpus green,
so the count is part of the gate's output rather than a debug aid.

**Follow-up, found 2026-10-06: the fuzzer had never been taught the declared
divergence.** `run.sh` normalises through `normalize.py`; `fuzz.py` compared raw
bytes, so at `c6fc271` `make verify` failed at the fuzz stage — 1 864 mismatches
in 2 000 cases on seed 1, every one of them the generator line — while
`make corpus` beside it passed 60/60. `fuzz.py` now imports
`normalize.normalise_bytes`, so there is one implementation of the rule rather
than two that can drift, and its summary prints `NORMALISED: <n>` unconditionally
for the reason above. Checked both ways: over 60 cases the shared rule gives 0
mismatches with `NORMALISED: 106`, and defaulting `caps_tag` to `B` instead of
`STRONG` still gives 6 mismatches with `NORMALISED` unchanged — the
canonicalisation does not swallow a real difference.

**Third finding from the same sweep:** 4 mismatches in 16 000 cases, all one
shape — upstream's own `tfiles/pre.txt` contains `file:Here`, the reference
links it, textrill refuses it. That is A11's scheme policy against a reference
that has no policy, declared Tier 2 with its own oracles
(`tests/urlschemetest.rs`, corpus case `opt_injection`), so the four dangerous
scheme tokens now leave the fuzzer's claimed input domain in `sanitise()` —
alongside non-ASCII and trailing whitespace — rather than entering a
known-divergence list. The three affected seeds re-ran at 0 mismatches with
`NORMALISED` unchanged.

### Items 1.2–1.4, resolved

**textrill uses its own names, and the old ones still work.**

- **1.2 — option files.** `rcfile::RC_NAMES` is `[.textrillrc, .txt2htmlrc]`.
  The preferred name is read if it exists and the legacy one otherwise, *within
  a directory*: precedence is still `@file` < home < cwd < command line, and
  never both files in one directory, because two array sources there would apply
  twice. Deduplication is by directory rather than by path, so `HOME=$PWD` is
  read once under whichever of its two names exists — the shape the reference's
  `home=>1, current=>1` has no protection against. `--help` documents the
  preferred names and says the legacy ones are still read.
- **1.3 — link dictionary.** `options::default_link_dict(home)` picks
  `~/.textrill.dict` when it exists, `~/.txt2html.dict` when only that does, and
  the textrill name when neither does (`HOME` unset: the same two names relative
  to the working directory). The option still points at one file, so this is a
  choice, not an accumulation.
- **1.4 — `--help`.** Line 1 is *"Convert plain text to HTML, inferring
  structure from layout. See the textrill README for details."* The reference is
  named only where it is factually relevant: the legacy option-file names.

Cost: the parity tier does not move, because no option spelling or default that
a corpus case depends on changed — corpus 60/60, goldens 33/33 after the change.

Gates added, each broken on purpose per § Standing rule:

| sabotage | expected | observed |
|---|---|---|
| `RC_NAMES` reversed to `[.txt2htmlrc, .textrillrc]` | rc-name tests red | 1 `optionstest` + 2 `rcfile` unit tests failed |
| `NAMES` reversed to `[.txt2html.dict, .textrill.dict]` | dict tests red | 3 of 4 failed; the legacy-only case passes under either order, by design |
| `--help` line 1 reverted to *"A reimplementation of txt2html 3.0"* | provenance red | `help_describes_textrill_and_not_the_reference` failed |

The tests are `tests/optionstest.rs` (P1.2, end to end), `src/rcfile.rs` and
`src/options.rs` unit tests (name selection and directory dedup), and
`tests/provenance.rs` (the help line, alongside the generator meta).

## Phase 2 — Fix what is wrong

All four were found by the investigation, and none is a regression: the first
two are byte-for-byte what the Perl original does, which is precisely why the
parity framing never surfaced them. Full reproductions in
`docs/CAPABILITIES.md` §4.

| # | item | risk |
|---|---|---|
| 2.1 | ~~Recognise an ordered list that does not start at 1~~ **withdrawn** | Low. Currently the structure is simply not found. Must not renumber `3.`/`7.` into `1.`/`2.` — the original numerals have to survive, so this needs `<li value>` or a stated policy. See "Items 2.1–2.2, withdrawn". |
| 2.2 | ~~A blank-line-separated ordered list after a bullet list stays a sibling~~ **withdrawn** | Low. Needs a corpus case that fails before the fix. See "Items 2.1–2.2, withdrawn". |
| 2.3 | ~~`--template` vs `--document_template`~~ **done** | Docs first: the names promise almost the same thing. Renamed: the body wrap is now `--body_template`; the legacy `--template` name still works and warns. See "Item 2.3, resolved". |
| 2.4 | ~~Definition lists need a real trigger~~ **done** | `term : definition` produces a plain paragraph today. Decide whether to document the existing trigger or make the obvious form work. Done: documented, and pinned against the reference. See "Item 2.4, resolved". |

### Items 2.1–2.2, withdrawn

Both fixes were implemented and validated — non-1 starts via `<li value>`, blank-line
siblings via a close-before-restart — against an oracle test and two corpus
NOGOLDEN cases, with the parity tier still green under the stated policy. They
were reverted because a Tier 2 *content* divergence and the fuzz gate are
mutually exclusive by design: `fuzz.py` is a strict byte differential over the
upstream `tfiles` seeds and has no mechanism to declare a content divergence
(its own comment explains why, citing E3). Every one of the 64 fuzz mismatches
(seed 1, from list-2/3, list-styles, list-advanced, list-custom, mixed, pre,
sample and heading1 mutations) traced to the two new list shapes, so `make
verify` cannot pass with a core-conversion divergence of this kind.

The outcome is a shrunken deliverable, not a lost one: the reference's behaviour
is confirmed and still recorded in `CAPABILITIES.md` §4.1–§4.2, and the
constraint on `<li value>` (numerals must survive, so a non-1 start needs an
explicit value) is documented there too. A future attempt at either item has to
come back with a gate that can declare a content divergence: the corpus already
can, via required-failure NOGOLDEN cases; the fuzzer cannot, by design.

### Item 2.3, resolved

**The two template names were a trap, and the body wrap has a real name now.**
`--template` (fragment inside `<body>`) and `--document_template` (whole page)
differed only by the word `document_`, and picking the wrong one silently
nested a complete document inside a `<body>` with two doctypes and two titles.
Templating is textrill's own feature — the reference has none — so the names
were ours to fix. The body wrap is now `--body_template`, the pair reads as
opposites in `--help`, and the legacy `--template` spelling is kept working (an
rc file may use it) but prints a deprecation warning naming both poles.

Gates: `optionstest` asserts the alias resolves to the renamed option and that
the help text names the pair and the deprecation; `templatetest` asserts the
alias still converts byte-identically and warns. The accepted-spelling count
moved 121 → 122, and `make verify` is green.

### Item 2.4, resolved

**The definition list has a trigger, and it is now a documented, pinned shape.**
A line that is exactly `term:` — a name of two or more word characters, a colon,
nothing after it — opens a `<dl>`, with the name as `<dt>` and the following
indented block as `<dd>`. It has worked since the first port and diverges from
the reference nowhere: both tools emit identical bytes for it. What was missing
was a decision and a record.

The one-line form `term : definition` stays a paragraph, deliberately. The
alternative — turning the obvious form into a definition list — is a
core-conversion *content* divergence: the reference emits `<p>term: definition</p>`,
and the same colon already marks an ordered-list item (`1: two`, `a: one`), so
every `Word: rest` line in real prose would change meaning, and the fuzzer
would flag it — the exact wall that withdrew 2.1–2.2. Documented in the
README's Lists section and in `CAPABILITIES.md` §4.6.

Gates: the `definitions` corpus case pins the emitted `<dl>` bytes and the
`<p>` boundary against the reference (differential, fixture-driven). Its
sabotage was observed: with the term trigger disabled, the case fails with
`ref : '<dl>'` / `mine: '<p>dpi:'`. `make verify` is green.

### Item 4.1, resolved

**`--var name=value` turns a fixed frame into something parameterisable.**
Repeatable, it binds `{{textrill:var:name}}` in the active template: the
settings a page or site build varies per run, as the homer.txt byline/date
example in `OFFERING.md` §5.1 needs. The value is inserted **verbatim** — the
author owns escaping, and a value that carries whole blocks (`<aside>…`)
survives byte for byte, not reflowed or squished against the engine's own
blocks. An **undeclared** `{{textrill:var:name}}` is a **hard error at
validation**, naming the missing variable and listing the declared ones, before
any output bytes exist — stricter than Mustache's silent empty, and the 4.4
unknown-slot guard is extended, not loosened (`var`, `varx`, `vary` stay
errors). An **empty value is legal** where a field is genuinely blank: the slot
contributes nothing and the surrounding markup stands — nothing invisible is
emitted behind the user's back; the token they wrote is where the value shows.
Substitution is **one pass**: a value containing `{{textrill:toc}}` stays
literal, so a parameter cannot smuggle the engine's own slots in or cascade
into them.

The name is letters, digits, `_` or `-`; the first `=` splits name from value
(`--var k=a=b` binds `k` to `a=b`). `--var` is parsed eagerly like `--encoding`
— a malformed one is a command-line error on the spot, not something a template
loads as an afterthought. `get_value`/`set_value` round-trip the joined form on
the `--infile` precedent, and setting the empty string is a no-op, so a front
end can persist the unset state.

Gates: `templatetest` asserts substitution in both template models, literal
insertion of a multi-line block, no re-scan of an inserted value, the
undeclared-name hard error (both with and without any `--var` declared), the
4.4 guard surviving, and eager CLI rejection of malformed `--var`; `optionstest`
asserts the `--var` round-trip and moves the counts 65 → 66 options and
122 → 123 spellings. Two sabotages were observed: (1) with the var slots not
pushed into `apply`, the substituted value test fails — `{{textrill:var:title}}`
is emitted literal; (2) with `validate` accepting any `var:` prefix, the
undeclared-name unit test and the `undeclared_var_slot_is_a_hard_error`
integration test both fail — the run exits 0 with a literal token in the
output. 4.4 stays green and `make verify` is green.

### Item 4.2, resolved

**Five actual templates ship with the tool, embedded in the binary.** `article`,
`book`, `manpage`, `slide` and `bare` are plain HTML files under
`textrill/templates/`, compiled into the static binary with `include_str!`, so
the library works from any directory with no install step and a user can still
read or copy any of them. `--template_library NAME` selects one; each name has
an intrinsic model — `article`, `book`, `manpage` and `slide` own the whole
document, `bare` wraps only the body and converts byte-identically to no
template at all — and the engine routes it exactly as the matching
`--body_template`/`--document_template` file would, so validation, the model
refusals, and the 4.4 guard are inherited, not re-implemented.

Two decisions recorded, because both look like compromises and neither is:

- **Fixed slots only, never `{{textrill:var:...}}`.** A shipped template with a
  var slot would force the user to `--var` it or see the S3 hard error on a
  first run; a var that silently emptied would be exactly the invisible frame
  S3 refuses to ship. Each template converts with zero required arguments and
  shows only what the document produces. The intended path to a byline or date
  is to copy the template and add `{{textrill:var:name}}` slots of your own —
  the library is a starting point, not a finish line.
- **`bare` is byte-identical to no template.** It documents the wrapper model
  and pins a real floor, rather than adding a cosmetic wrapper that would make
  "templated" and "untemplated" diverge for no structural reason.

Gates: the library's own unit tests validate every shipped template against
the engine (`template::validate` passes with zero declared vars — so no unknown
slot, `content` present, and any var slot would fail here) and assert the
documented five-with-models; `templatetest` converts a document through every
shipped template (document templates start with a doctype and carry the
converted body; `bare` matches the untemplated bytes), rejects an unknown name
as a hard error naming the library, checks the mutual exclusion with
`--body_template`/`--document_template`/the legacy `--template`, and checks the
model refusals (`--extract`/`--chunk`/`--stream` for both models,
`--prepend_file` only for the document model). Two sabotages were observed:
(1) `book.html` without its `{{textrill:content}}` fails the validate-every-
template unit test and the book conversion test; (2) an unknown name that
silently resolved to a template fails both the unknown-name unit test and the
hard-error integration test. 4.4 stays green and `make verify` is green.

## Phase 3 — HTML5 by default

XHTML 1.0 Strict is the Perl default and is wrong for a new tool in 2026. Emit
HTML5 and a charset meta unless told otherwise; keep XHTML available. This is a
deliberate divergence from the reference and should be recorded as one, with the
corpus cases that assert HTML4 exempted rather than deleted.

**Done (S5), precondition first.** The planned exemptions turned out to be
unnecessary, because the stronger fix is to pin the doctype *on both sides* so
no case reads a default: `run.sh` constructs the reference with
`HTML::TextToHTML->new('xhtml' => 1, @ctor)` — the module's own default made
explicit and placed first, so per-case `CTOR`/`EXTRA` (`sample`, `empty1`,
`empty3`) still override — and invokes the port with `--xhtml` ahead of the
case's own flags, so later flags still win; `fuzz.py` puts exactly one of
`--xhtml`/`--no-xhtml` at the front of every generated case and drops the pair
from the random option pool, since a fuzzer samples rather than declares. With
the pins in, the flip moved zero cases: PASS stays 61/61 and all 33 goldens
stay byte-identical (the eight encoding cases and `opt_injection` that must
differ predate this and are unrelated). The flip itself: defaults become
`html5: true`, `lower_case_tags: true`, `xhtml: false`; `--html5` and `--xhtml`
are each other's complement and carry their mode's tag case, so `--no-html5`
lands on the reference's HTML 4 *with* its upper-case tags, while an explicit
`--lower_case_tags` given after the flag still wins and `--no-lower_case_tags`
is honoured in the default HTML 5 mode (the reference never turns it off —
recorded divergence, not emulation); `do_file_start` checks `xhtml` before
`html5` so a post-construction mutation still wins; the HTML4 branch keeps the
reference's bytes exactly. The settings blob needed the matching fix: it is a
full-state snapshot, so `options_store::apply` assigns the two doctype
booleans directly rather than replaying the CLI's transition arms (which would
read `xhtml: false` as `--no-xhtml` and clear `html5` out from under the blob).
Gates: `provenance` asserts the default document carries exactly one
`<meta charset="utf-8">` and that legacy mode carries none, `html5test`
asserts the default starts `<!DOCTYPE html>` without a namespace and that one
flag reaches the reference's XHTML, and the GUI/CLI tests follow the new
default. Sabotage observed: restoring the old defaults
(`xhtml: true, html5: false, lower_case_tags: false`) turned `html5test` (3
failures), `provenance` (2) and positioning P8 (bytes: documented 38 477,
measured 38 882) red while the pinned corpus stayed 61/61 green — which is
the reason the pins are the precondition and not an afterthought.

## Phase 4 — Templates, which is where we are behind

`docs/LANDSCAPE.md` §3: textrill is ahead on reading prose and behind on output,
and a user's first impression of a converter is its templates. The namespaced
7-slot design stays — passing other engines' `{{ }}` through untouched is
correct and is not what needs changing.

| # | item | note |
|---|---|---|
| 4.1 | ~~`--var name=value` → `{{textrill:var:name}}`~~ **done (S3)** | The single highest-value addition. Turns a fixed frame into something parameterisable. |
| 4.2 | ~~Ship actual templates: article, book, manpage, slide, bare~~ **done (S4)** | Content, not machinery. This is the "templates people could use" the niche is named for. |
| 4.3 | `{{textrill:if:name}}…{{textrill:end}}` | One template serving documents with and without a TOC. |
| 4.4 | Unknown `textrill` slot stays an error; other engines' tokens still pass through | Existing behaviour. Do not regress it. |

## Phase 5 — Structure in documents that have none

The finding that motivates all of this.

`examples/homer.txt` has no headings: the engine turns the 39 capitalised runs
it can see into `<strong>`, `--toc` lists nothing, and `--chunk` writes a single
38 477-byte file. Pandoc finds none of it either, for a different reason.

What the file actually contains, measured 2026-10-06 rather than assumed — and
it is not what this phase first assumed when it claimed "37 obvious section
titles":

- **40 all-caps lines**, of which **24 are the contents list** (lines 5–31:
  consecutive lines inside one block, entries for sections this extract does not
  contain). Turning those into headings would invent a section per contents
  entry.
- **Three genuine section starts**: `PREFACE TO FIRST EDITION`,
  `PREFACE TO SECOND EDITION`, and the title block
  `THE ODYSSEY` / `BOOK I` / `THE GODS IN COUNCIL—…` — three consecutive caps
  lines, not one line alone between blanks.
- **The false positives are in the same shape as the truth**: `S. BUTLER.`,
  `HENRY FESTING JONES.` (signatures) and `120 MAIDA VALE, W.9.` (an address)
  are all-caps and start a block exactly as a heading would.

**The candidate rule, measured on the document that motivated it.** "A short
all-caps line alone between blank lines" fires on **2 lines — both signatures,
none of the three real titles**. Widened to "a block whose every line is short
and caps" it fires on 3: the two signatures and the title block. Widened again
to "a block that *starts* with a caps line" it fires on 6: 3 true, 3 false.
Every simple formulation is empty or half false on the motivating document, so
the measurement this phase already required has effectively started, and its
first result is that the rule as written would not fix what it was written for.

**Measured over `examples/`, 2026-10-08 (S8), `make measure`.** The three
candidate rules applied to the whole eight-document corpus — all-caps lines that
are alone between blanks (R2a), blocks whose every line is short and caps (R2b),
blocks that start with a caps line (R2c) — plus the setext-underline rule as the
baseline the engine already has. Truth labels: homer's three genuine section
starts (lines 38/78/102), Gelbenhügel's ten literal markdown headings (the tool
cannot see them, which is itself a result), and zero for the six no-structure
documents, so every hit there is a false positive. Cells are catches/false
positives:

| doc | setext | R2a | R2b | R2c |
|---|---|---|---|---|
| homer.txt | 0/0 | 2/2 | 2/2 | 6/3 |
| gelbenhuegel.txt | 16/16 | 0/0 | 0/0 | 0/0 |
| blake.txt | 0/0 | 27/27 | 27/27 | 27/27 |
| calli.txt | 0/0 | 158/158 | 158/158 | 159/159 |
| erya.txt | 0/0 | 0/0 | 0/0 | 0/0 |
| mohe_zhiguan_vol001.txt | 0/0 | 0/0 | 0/0 | 0/0 |
| septuagint_swete_genesis.txt | 0/0 | 0/0 | 0/0 | 0/0 |
| talmud.txt | 0/0 | 0/0 | 0/0 | 0/0 |

The table corrects the plan's own first pass: **R2b was published as 3, measured
it is 2** — the title block has a long third line (`THE GODS IN COUNCIL—…`), so
it fails "every line short and caps", and both hits are the two signatures.
R2a confirms the earlier read (both hits false); R2c reproduces 6/3 exactly.

**The result the measurement exists for:** on prose that is not structured,
every candidate rule's false-positive count is its full hit count — 100% on
Blake and Calligrammes, 100% on the signatures, and the setext baseline is
already finding 16 things Gelbenhügel's author never wrote (markdown `---`
separators read as underline rules; the tool also misses all ten real `##`
headings). The phase gates matter here: no rule is near zero false positives on
real documents, so this stays a proposal. R2b's disqualification is not noise —
the "every line short and caps" unit was what made it *look* less noisy than
R2a.

**The candidate rule, restated for the next pass:** the unit is probably the
*block* rather than the line — a title block, a contents run and a signature are
all caps runs, and what separates them is position (does a section follow?)
rather than case. That is a hypothesis to measure over `examples/`, not a
decision; Phase 7 grows the corpus for exactly this reason.

**Why none of it is obviously safe, and must be measured first:**

- Shouting in prose is real. `HE SAID NO.` at the start of a line becomes a
  heading under the line rule.
- The caps heuristic already owns this text. The two features collide, and one
  has to yield.
- `min_caps_length`, `short_line_length` and `custom_heading_regexp` all become
  load-bearing for a decision nobody asked to be configurable.
- `custom_heading_regexp` only fires on a line that *starts* a block, which
  measurement found the hard way: on `homer.txt`, `-H 'PREFACE'` yields 2
  headings, `-H 'THE ODYSSEY'` yields 1, `-H 'BOOK I'` yields 0 — the body's
  `BOOK I` is the second line of its title block and the contents' `BOOK I.` is
  one of 24 consecutive lines. No regexp can select a line the rule will not
  consider, and neither `--help` nor the README says so.

| # | item | note |
|---|---|---|
| 5.0 | ~~`--report`: the inference counts on stderr~~ **resolved (S6)** | Report-only, and first. The counts exist today only in `make examples`, which greps the output itself; the CLI offers no `--report`, `--verbose` or summary (verified in `--help`). A user whose `--toc` came out empty, and this phase's own measurement harness, both need "what did you see?" before anything is allowed to change. See "Item 5.0, resolved". |
| 5.1 | ~~Measure candidate rules over `examples/`~~ **resolved (S8)** | The table above is the first data point. Required before any behaviour change, as below. | `make measure` pins the per-rule false-positive table over the eight documents; the verdict is recorded above — no rule is near zero false positives, so Phase 5's heading change stays a proposal. |
| 5.2 | Decide what `-H`'s block-start condition should be | Either document it in `--help` and the README, or change it — measured, not silently. Changing it means a heading rule can consume a line from the middle of a paragraph, which is the same hazard as 5.1's false positives. |

Before writing any of it: build the measurement set over `examples/` — how many
candidate headings, how many false positives on prose that is not structured. If
the false positive rate is not near zero on real documents, this stays a
proposal. The archived plan's own advice applies — measure first, and prefer
report-only before behaviour change, which is what 5.0 is for.

### Item 5.0, resolved

**`--report` prints what the conversion recovered, on standard error, after the
output is written**: `bytes`, `headings` (`<h1>`–`<h6>`), `paragraphs`,
`strong` (capitalised runs) and `br` (line breaks), as one `key=value` line the
way `make examples` prints them:

```text
textrill: report bytes=38477 headings=0 paragraphs=64 strong=39 br=34
```

Four decisions recorded, because each is the shape of a trap:

- **The counts are of the produced tags, not of emission events.** The engine
  truncates a `<p>` wrapper that already lost its content (`notes.rs` empties it)
  while the paragraph still contributed its `<p>` tag at the moment it was
  emitted, so an event counter would report paragraphs the file does not contain.
  A count the file does not agree with is the bug this item exists to prevent, so
  the scanner reads the finished output — which is what a user with the file can
  reproduce with `grep`.
- **Counting is case-insensitive.** `--no-html5` emits the reference's
  upper-case tags; a report that reads 0 headings off a valid HTML 4 document
  would be reporting the serialisation rather than the structure.
- **`paragraphs` is `<p` followed by `>` or whitespace**: `<p class=…>`
  (mailmode) and `<p>` count, `<pre>` does not. That is the one rule a naive
  `<p` prefix gets wrong, and the sabotage below is built on it.
- **`make examples` now reads `--report` instead of `grep`ping**, so the CLI
  count and the smoke-run count are one implementation and cannot drift. The
  independence has to come from somewhere, so `reporttest` recounts the output
  with a deliberately different implementation (lowercase + `matches`, where the
  product scans bytes).

Refused with `--stream`, which never assembles a finished document to count —
the same class of refusal as the whole-body passes. The output is byte-identical
with and without the flag; only stderr differs. With `--chunk` the line totals
the run, summing the files written.

`--report` has no reference equivalent: the wrapper's `GetOptions` declares
`verbose!` but never reads it, so there is nothing to be byte-par with.

Gates: `reporttest` (9 tests) asserts the flag is off by default, prints only on
stderr, leaves the output byte-identical (stdout and file paths), matches an
independent recount across `<pre>`-bearing, mailmode and upper-case inputs,
totals `--chunk` runs, refuses `--stream`, and pins exactly the numbers `make
examples` prints for `examples/homer.txt`; the `report` module's unit tests pin
the individual rules; `optionstest` moves the counts 67 → 68 options and
124 → 125 spellings; the GUI's widget-coverage test moves 67 → 68 (`report`
appears as an inert checkbox); `positioning.py` P16 moves 67 → 68. Two sabotages
were observed, both before the fix was believed: (1) with the paragraph rule as
a naive `<p` prefix, `an_indented_block_is_not_a_paragraph` and
`the_counts_are_what_is_in_the_output` fail — and they had to exist, because
homer.txt has no `<pre>` at all, so the absolute homer gate alone would not have
noticed; (2) with the report written to stdout instead of stderr, six tests
fail, because the output now differs. `make verify` is green.

## Phase 6 — Packaging

Flatpak manifest is drafted at `packaging/io.github.youtubescholar.Textrill.yml`
and is deliberately unbuildable: `packaging/cargo-sources.json` is generated by
`make cargo-sources`, which needs `flatpak-cargo-generator`, and whether to
generate or `cargo vendor` is undecided. The app-id is settled
(`io.github.youtubescholar.Textrill`, following the GitHub owner resolved
2026-10-08). Details in `docs/PACKAGING.md`.

## Phase 7 — The example corpus

`examples/` now holds eight documents; `make examples` reports recovered
structure for each (the counts are recorded in the same pass and pinned in
`reporttest`). The provenance and licence of every file, checked at
acquisition time, live in `examples/README.md`. Still worth adding, in rough
order of value:

- a document with real punctuation-run headings, to exercise `-H` on non-markup
  structure with a different sign than `====`/`----`
- a document with tables, to exercise `--make_tables` on real data
- an email thread, for `--mailmode`
- a deliberately hostile document, to confirm the URL-scheme policy

---

## Phase 8 — Independence: retire the reference as a gate

`docs/ORACLE-ARCHAEOLOGY.md` measured the question this phase answers: of every
bug the project found in itself, the Perl differential was the *unique*
discoverer of the semantic cases a reference-free oracle cannot judge — E3's
blank lines, `delim_retry`, and the non-ASCII delimiter predicate — and every
one is now frozen as a corpus case or regression test. The differential's
remaining job is regression, and a frozen golden or a property does that
without its limitation: it can say *different*, never *wrong*.

The gate also has a cost that is easy to forget because it is quiet. The fuzz
stage is a strict byte differential with no way to declare a *content*
divergence; Phase 2.1–2.2 recorded the wall (64 mismatches, all from two correct
list shapes). It is why those items were withdrawn and why Phase 5's heading
change is pinned behind a measurement. A gate that forbids improvement is not
only a gate.

This is the bootstrap compiler being retired. The ancestor built the descendant
and found the three things only it could see; once those are frozen, it is not
needed to decide pass/fail.

### What the differential guards, and its replacement

Removing the oracle before a replacement exists turns a real gate into a green
gate that checks less, so S12 builds the replacement first.

| guarded today | replacement | note |
|---|---|---|
| 61 byte-identical `tfiles` cases | hand-reviewed self-goldens | reviewed, not blindly frozen: a frozen bug is not a gate |
| 33 author `good_*.html` goldens | unchanged | already reference-free |
| 10 `differential must fail:` cases | plain expected-output tests | the "declared divergence" category then has nothing to declare |
| E3, `delim_retry`, non-ASCII delimiter semantics | frozen self-goldens | the three cases Perl alone found |
| random option combinations (fuzz, 8 seeds) | **partially lost** | see below |

The fuzzer is the honest gap: it samples real options at random and compares
fresh Perl output, so no fixed golden replaces it one-for-one. The property
suite (`proptest.py`, `alloctest.rs`) covers the failure classes — no data loss,
well-formedness, determinism, resource bounds — but the random-option *content*
coverage shrinks. That is accepted deliberately and recorded here rather than
discovered later.

Desired end state: `make verify` no longer invokes Perl; `make diff` runs the
same differential and fuzz runs as a non-gating cross-check; `ref/`, the
`YAML::Syck` stub and the `perl` CI job become an archived historical tool,
dropped in a later change if nothing needs them.

### Steps

- **S12 — build reference-free acceptance.** Freeze reviewed self-goldens for
  `tfiles/` and `examples/`; convert the ten `differential must fail:` cases to
  plain expected-output tests; grow the truth sets (`measure.py`'s per-document
  counts, `encodingtest.rs`'s decoded code points). The differential runs in
  parallel throughout as a cross-check: a new self-golden that disagrees with
  fresh Perl is a finding to review, never an edit to make green. S12 changes
  the harness, so every new gate is broken on purpose before it is believed —
  CONTRIBUTING.md rule 2 and the Standing rule, not an exception to them.
- **S13 — flip the gate and sweep the documents.** Only after S12 is complete
  and reviewed: drop the `perl` differential and fuzz stages from `make verify`,
  add the non-gating `make diff`, retire the `perl` CI job — and rework the
  `musl` job, which also runs `corpus-musl` against the reference. Then sweep
  the documents below, because the tree would otherwise document a gate it no
  longer has. `make verify` is green with no Perl installed.

Until both land, the reference remains a parity oracle exactly as "What this
is" and "Sequencing" describe. Phase 8 is the plan to stop citing it as one.

### Documentation sweep (S13)

The differential is not just a target; several documents describe it as the
load-bearing gate. When S13 removes it, each of these has to move in the same
change or it becomes documentation for a mechanism that is gone:

| file | what stops being true |
|---|---|
| `CONTRIBUTING.md` | rules 2–3 name the differential corpus as *the* pinning mechanism; the gates block lists corpus/fuzz in `make verify` |
| `README.md` (root) | `make ref` as required setup, "the only thing that gets to say behaves like upstream", `perl` as a prerequisite |
| `textrill/README.md` | the byte-parity contract in "Status" and "Relationship to upstream"; `make ref` in the build steps |
| `docs/OFFERING.md` | §9's gate table and the §1 "Checks" row list corpus/fuzz inside `make verify` |
| `docs/CAPABILITIES.md` | the appendix's corpus command and "pinned against the reference" phrasing |
| `DOCS.md` | the `ref/` entry's "oracle for the parity tier" framing |
| `.github/workflows/ci.yml` | the `differential` job, the upstream canary, and `musl`'s `corpus-musl` step |
| `textrill/tests/corpus/README.md` | the whole document is written around the differential; reduce it to the advisory `make diff` and the reference-free acceptance |

---

## Not doing

- **Footnotes.** Every mature converter has them. Rejected: footnotes need
  unambiguous inline markers, and the one thing this tool must never do is guess.
  A `[^1]` in ordinary prose becoming a link is worse than not having footnotes.
- **Multi-target output.** txt2tags ships 18, pandoc more. That would be a
  different product.
- **Syntax highlighting in preformatted blocks.** A language-detection
  dependency and a large behavioural surface inside blocks that are currently
  literal text.
- **Reproducing Perl's behaviour where Perl is wrong.** The encoding bug is the
  standing example.

## Sequencing

The phase sections above are topical; this section is the only ordering. One
list, and each step names the check that proves the change did not destabilise
the application. Every step starts `make verify`-green and ends the same way,
and each step's new gate is broken once, on purpose, before it is believed
(Standing rule).

Baseline for every step: `make verify` (fmt, clippy, tests, corpus, fuzz); `make
proof` too, wherever the offer's numbers are on the line. Each step adds its own
oracle on top of the baseline.

One constraint on every step: the Perl reference is a *parity oracle*, not a
design authority. textrill documents its own semantics — a feature is introduced
or explained in textrill's own terms, with its own justification, and the
reference is cited only where it constrains the bytes (as it did for 2.4's
one-line boundary). Behaviour that diverges still has to clear the differential
gates on the reference's terms, but the rationale is textrill's, never "the
original does it too". Phase 8 (S12–S13) ends this constraint: it replaces the
parity oracle with reference-free acceptance and demotes the reference to an
optional cross-check.

| step | item | why here | check (beyond the baseline) |
|---|---|---|---|
| S1 | ~~Phase 2.3 — `--template` vs `--document_template`~~ **done** | The surviving "fix what is wrong" items; still correct to do next | renamed: `--body_template` is the body wrap, the legacy `--template` warns; `optionstest` + `templatetest` assert it; verify green |
| S2 | ~~Phase 2.4 — definition-list trigger~~ **done** | Same | documented, and pinned: the `definitions` corpus case (term trigger + `<p>` boundary) fails against the reference when the trigger breaks; verify green |
| S3 | ~~Phase 4.1 — `--var`~~ **done** | Highest user-visible value; OFFERING §5 row 2 | substitution + undeclared-name gate broken once (see "Item 4.1, resolved"); the 4.4 unknown-slot guard stays green |
| S4 | ~~Phase 4.2 — shipped templates~~ **done** | "templates people could use" | a conversion test per shipped template; 4.4 guard green; the Item 4.2 gates |
| S5 | ~~Phase 3 — HTML5 default~~ **done** | A deliberate *option-default* divergence; the doctype interacts with the differential | precondition pins (corpus both sides, fuzz pair mandatory) made the exemptions unnecessary and the flip moved no case; provenance asserts the HTML5 charset meta; sabotage observed — old defaults fail html5test/provenance/P8 while the pinned corpus stays green |
| S6 | ~~Phase 5.0 — `--report`~~ **done** | Report-only instrument; prerequisite to any heading change | `--report` prints the five counts on stderr `key=value`; `make examples` reads them from that line instead of `grep`ping, so the two cannot drift; `reporttest` recounts the output with an independent implementation; sabotage observed — a naive `<p` prefix overcounts `<pre>` blocks, and a report on stdout pollutes the output (see "Item 5.0, resolved") |
| S7 | ~~Phase 7 — grow `examples/`~~ **done** | 5.1 measures over real documents; one document cannot support a false-positive rate | eight documents, each CC0/PD-confirmed with source and edition in `examples/README.md`; all recovered-structure counts recorded by `make examples` and pinned in `reporttest`; verify green |
| S8 | ~~Phase 5.1 — measure candidate rules~~ **done** | Required before any Phase 5 behaviour change | `make measure` re-derives the R2a/R2b/R2c and setext-baseline counts over all eight documents and pins the table; verdict written into Phase 5 (no rule near zero false positives; R2b corrected from 3 to 2); no behaviour change this step; verify green |
| S9 | Phase 5.2 — decide `-H` block-start | The only Phase 5 behaviour change, once the measurement allows it | document the condition in `--help`/README, or change it with new corpus cases + oracle; verify green |
| S10 | Phase 6 — packaging | Independent; owner resolved (`youtubescholar` 2026-10-08), remaining blocker is generate-vs-vendor | a Flatpak build from `make cargo-sources` (or `cargo vendor`) succeeds; app-id `io.github.youtubescholar.Textrill` |
| S11 | Phase 4.3 — `{{textrill:if:…}}` | After S4's library gives it something to condition on | template tests extended; 4.4 guard green |
| S12 | ~~Phase 8 — build reference-free acceptance~~ **done** | The replacement must exist before the gate it replaces is removed | self-goldens for `tfiles/`+`examples/` captured and cross-checked against the differential and the author goldens (`SELF 61/61`, `AUTHOR 33/33`, `examples 8/8`); the ten declared-divergence cases are plain self-golden cases; truth sets grown in `encodingtest.rs` (verified --decode of the encoding fixtures) and the example frozen outputs; differential ran in parallel throughout, then `make verify` green with the new gates; each new gate broken on purpose (Standing rule) |
| S13 | ~~Phase 8 — retire the reference as a gate, and sweep the documents~~ **done** | Only once S12 is complete and reviewed | `make verify` green with no working `perl` on `PATH` (proven with a poisonous shim); `make diff` added (`ref corpus fuzz`) and non-gating; the `differential`/`perl` CI job retired and `musl` reworked to `accept-musl`; the Documentation sweep table below is clear; full 8-seed `make diff` cross-check green |

`OFFERING.md` §5 is the same work ordered the way a first-time reader meets it
(proof, 4.1–4.2, 5.0, headings, packaging, corpus, `if`) — not a second
authority, which is exactly where the old sequencing slipped: it claimed the two
orders "agree", but it skipped Phase 2's survivors and Phase 3 (engine items,
not offer rows) and could not see that Phase 5's measurement needs Phase 7's
growth first. Where the reader order and this list differ, this list governs.

## Standing rule

A gate that has never been observed failing is not a gate. Every new check in
this plan gets broken on purpose and confirmed to exit non-zero before it is
believed — the rule recorded in `legacy-archive/REMEDIATION-PLAN.md` Phase 0b,
where three separate checks were found printing success while being incapable of
reporting failure.
