# textrill — plan

Written 2026-10-06. Replaces `legacy-archive/REMEDIATION-PLAN.md`, which
answered a different question. That plan asked what the Perl original did that
this port did not, and its 65-option inventory existed to produce a list of
gaps. Producing that list is what kept the work pointed at mimicry, and it hid
the things that were actually wrong.

This plan is built from `docs/CAPABILITIES.md` (what the tool does, measured) and
`docs/LANDSCAPE.md` (who else does it). Every item below traces to a measurement
in one of those two documents.

---

## What this is

textrill converts **plain text** to HTML. The input is not required to carry any
markup language; structure is inferred from layout, and the result is placed
into HTML through templates the user controls.

**Licensing is settled and not revisited.** GPLv3, with the Perl original
credited to Seth Golub, Kathryn Andersen and Joao Eriberto Mota Filho. That text
in `textrill/LICENSE` is correct and stays.

**The Perl reference stays available** at `ref/` as an oracle for the parity
tier of the test harness and as a historical artifact. It is no longer the
specification. Where the two disagree, the question is now "which is right",
answered per input class, and answered by us.

---

## Principles

1. **No markup required.** That is the product. Anything that makes the user
   mark the document up first is a different tool.
2. **Never drop text.** Declining to infer structure is fine. Dropping or
   silently reordering characters is not.
3. **Infer, then say so.** `make examples` reports what was recovered, so a
   regression in inference is a changed number rather than a shrug.
4. **The user's template is the user's.** No imposed page, no theme.
5. **Errors are errors.** Non-zero exit, nothing half-written.

---

## Phase 1 — Give the tool its own name

Nothing here changes behaviour. All of it is the port still wearing the Perl
module's identity, which is embarrassing in every document produced and wrong in
the `generator` metadata.

| # | item | why |
|---|---|---|
| 1.1 | `generator` meta says `textrill`, not `HTML::TextToHTML v3.0` | `convert.rs:617` hard-codes the Perl module. Every file the tool writes claims Perl made it. Attribution belongs in `LICENSE`, not in provenance. |
| 1.2 | Read `~/.textrillrc` and `./.textrillrc`; keep `.txt2htmlrc` working | Same precedence order, new preferred names. |
| 1.3 | Read `~/.textrill.dict` / `.textrill.dict`; keep the old names | Same. |
| 1.4 | `--help` line 1 describes textrill | Currently: *"A reimplementation of txt2html 3.0"*. |

Item 1.1 is the one that matters. 1.2–1.4 are compatibility-preserving, so the
parity tier should stay green.

## Phase 2 — Fix what is wrong

All four were found by the investigation, and none is a regression: the first
two are byte-for-byte what the Perl original does, which is precisely why the
parity framing never surfaced them. Full reproductions in
`docs/CAPABILITIES.md` §4.

| # | item | risk |
|---|---|---|
| 2.1 | Recognise an ordered list that does not start at 1 | Low. Currently the structure is simply not found. Must not renumber `3.`/`7.` into `1.`/`2.` — the original numerals have to survive, so this needs `<li value>` or a stated policy. |
| 2.2 | A blank-line-separated ordered list after a bullet list stays a sibling | Low. Needs a corpus case that fails before the fix. |
| 2.3 | `--template` vs `--document_template` | Docs first: the names promise almost the same thing. Then rename or warn. |
| 2.4 | Definition lists need a real trigger | `term : definition` produces a plain paragraph today. Decide whether to document the existing trigger or make the obvious form work. |

## Phase 3 — HTML5 by default

XHTML 1.0 Strict is the Perl default and is wrong for a new tool in 2026. Emit
HTML5 and a charset meta unless told otherwise; keep XHTML available. This is a
deliberate divergence from the reference and should be recorded as one, with the
corpus cases that assert HTML4 exempted rather than deleted.

## Phase 4 — Templates, which is where we are behind

`docs/LANDSCAPE.md` §3: textrill is ahead on reading prose and behind on output,
and a user's first impression of a converter is its templates. The namespaced
7-slot design stays — passing other engines' `{{ }}` through untouched is
correct and is not what needs changing.

| # | item | note |
|---|---|---|
| 4.1 | `--var name=value` → `{{textrill:var:name}}` | The single highest-value addition. Turns a fixed frame into something parameterisable. |
| 4.2 | Ship actual templates: article, book, manpage, slide, bare | Content, not machinery. This is the "templates people could use" the niche is named for. |
| 4.3 | `{{textrill:if:name}}…{{textrill:end}}` | One template serving documents with and without a TOC. |
| 4.4 | Unknown `textrill` slot stays an error; other engines' tokens still pass through | Existing behaviour. Do not regress it. |

## Phase 5 — Structure in documents that have none

The finding that motivates all of this.

`examples/homer.txt` has 37 obvious section titles — `PREFACE TO FIRST
EDITION`, `BOOK I.` … `BOOK XXIV.` — set in capitals, on their own line,
surrounded by blank lines. textrill finds all 39 capitalised runs and calls them
`<strong>`. It finds no headings, `--chunk` emits a single 38 KB page, and
nothing about the budget idea in the archived plan would change that, because
there is no boundary to cut on. Pandoc finds none of it either, for a different
reason.

**The candidate rule:** a short all-caps line alone between blank lines is a
heading, not a `<strong>` run. This is txt2tags' inference done in reverse —
inferring the underline instead of requiring it.

**Why it is not obviously safe, and must be measured first:**

- Shouting in prose is real. `HE SAID NO.` at the start of a line becomes a
  heading under this rule.
- The caps heuristic already owns this text. The two features collide, and one
  has to yield.
- `min_caps_length`, `short_line_length` and `custom_heading_regexp` all become
  load-bearing for a decision nobody asked to be configurable.

Before writing it: build a measurement set over `examples/` — how many candidate
headings, how many false positives on prose that is not structured. If the false
positive rate is not near zero on real documents, this stays a proposal. The
archived plan's own advice applies — measure first, and prefer report-only before
behaviour change.

## Phase 6 — Packaging

Flatpak manifest is drafted at `packaging/io.github.example.Textrill.yml` and is
deliberately unbuildable: the app-id is a placeholder because the reverse-DNS
domain depends on a GitHub account that does not exist yet, and
`packaging/cargo-sources.json` is generated by `make cargo-sources`, which needs
`flatpak-cargo-generator`. Whether to generate or `cargo vendor` is undecided.
Details in `docs/PACKAGING.md`.

## Phase 7 — The example corpus

`examples/` holds one document. `make examples` reports recovered structure for
each. Grow it, and the growth is the point: a converter is only trustworthy on
the documents its users actually have. Worth adding, in rough order of value:

- a document with real markup headings (`====`/`----`)
- a document with tables, to exercise `--make_tables` on real data
- an email thread, for `--mailmode`
- a non-English document, to exercise encoding beyond UTF-8
- a deliberately hostile document, to confirm the URL-scheme policy

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

Phase 1 first: it is small, it is uncontroversial, and it is embarrassing to ship
anything else while it is true. Phase 2 next, with corpus cases that fail before
each fix. Phase 4 is the largest chunk of user-visible value. Phase 5 is the
research-shaped one and must not be rushed into a behaviour change.

## Standing rule

A gate that has never been observed failing is not a gate. Every new check in
this plan gets broken on purpose and confirmed to exit non-zero before it is
believed — the rule recorded in `legacy-archive/REMEDIATION-PLAN.md` Phase 0b,
where three separate checks were found printing success while being incapable of
reporting failure.
