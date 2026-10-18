# Textrill — Scope and Framing (v1, working draft)

> Status: **braindump / pre-decision.** Not shipped documentation, not a
> commitment. The user contract is `textrill/README.md`; this file works out
> *what Textrill is for, what it deliberately is not, where it sits, how to use
> it well, and how we would know it works.* Expect it to be rewritten.
>
> Written for a mixed room: funder, scholar, archivist, general user,
> contributor, system engineer, reviewer. It is sequenced to be read straight
> through.

---

## 1. What Textrill is

*For: everyone.*

### 1.1 Thesis

Textrill is a **deterministic, offline converter from plain text to HTML**. It
infers structure from the conventions authors already use — blank lines for
paragraphs, indentation for lists, underlines and setext rules for headings,
trailing punctuation for link labels — so no markup has to be authored or
assumed. Its distinguishing feature is not the markup it produces but the
**guarantees around the conversion**: nothing is silently dropped, the output
is well formed, the same input yields the same bytes, and memory is bounded.
Those guarantees *are* the product. The HTML is the artifact; the trust is the
point.

### 1.2 The gap it fills

Plain text is the **format of record**: it survives migrations, can be read by
eye, diffed, hashed, and stored forever at negligible cost. HTML is the
**format of use**: what you read, link to, cite by anchor, search, and feed to
a machine.

The step between them is where things usually go wrong. Markup-first tools
demand structure a human (or an OCR pipeline) must author and that may never
have been there. Generative tools produce plausible HTML that may quietly
invent or delete content. Either way, the conversion is the risky,
unverifiable step.

Textrill's position: the conversion should be **mechanical, auditable, and
repeatable**, and its failure modes should be impossible or loud rather than
silent.

### 1.3 What it is

- A **layout-inference engine** for prose written to be read by a person, not
  parsed by a machine.
- A **CLI** (`textrill`, a single static binary) and a **native GUI**
  (`egui`/`eframe`; no Python, no Qt).
- A **library** (a Rust crate you can embed).
- A **port** of `HTML::TextToHTML` / `txt2html` (Perl) that keeps the legacy
  byte-parity modes as a cross-check; the default output has moved on (HTML5,
  closed optional tags, reworked encoding detection).

### 1.4 What it is not (non-goals)

All deliberate.

| Not this | Why not |
|---|---|
| A markup language | The input is plain text. To author tags, use HTML. |
| A WYSIWYG editor / renderer | It converts; it does not lay out pages or paint pixels. |
| OCR / image text extraction | Runs *after* capture. Pixels→characters is a different, upstream tool. |
| A reverse converter (HTML→text) | **There is no path back.** See §2.2. |
| Format-byte-lossless | Paragraphs reflow; the *words* survive, line breaks and spacing need not. |
| A document manager / CMS | No database, library UI, or search index. |
| A template language | Named slots only — no loops, conditionals, expressions, includes. |
| An AI / model | No inference, no network, no GPU. Substrate an AI can safely stand on, not a competitor. |
| A formatter for arbitrary rich documents | Complex tables, multi-column layout, embedded media are out of scope. |

### 1.5 Why it matters

- **Scholars and archivists:** annotations, citations and glosses live in the
  text as explicit, namespaced markers that become real endnotes and
  definition lists — with *no inference from prose* and a **hard error** on a
  dangling, duplicated, or unbalanced marker rather than a silent drop.
- **Trustworthy to machines:** plain text in, deterministic HTML out, with
  stable `id` anchors (`chunk-N`, `note-ref-*`, `gloss-*`). A generated edition
  can be cited by anchor and the citation resolves.
- **Memory and compute savings:** `--stream` holds **one paragraph**, not the
  document; the engine is CPU-only with a fixed ~2 MB working set; a converted
  edition is tens of kilobytes where its page images are tens of megabytes. See
  §5.

---

## 2. What it guarantees

*For: engineers, reviewers, system engineers. Everyone else can skim the tables.*

### 2.1 Non-negotiables, with evidence

Gated in CI/`make verify`, separately from byte-goldens, so they cannot regress
whatever the output bytes become.

| Guarantee | Statement | Enforcement |
|---|---|---|
| **No dropped text** | Every word token in the input appears in the output | `tests/proptest.py` property 1 (`WORD_RE = [0-9A-Za-zÀ-ɏ一-鿿]+`) |
| **Well-formed output** | The HTML parses; tags balanced/nested | `proptest.py` |
| **No panic on hostile input** | Random/edge input never aborts the process | `proptest.py` |
| **Determinism** | Same input → byte-identical output | corpus goldens (`accept.sh`, `SELF: 63/63`), `provenance.rs` |
| **Bounded resources** | Allocation is linear; no per-conversion retention | `alloctest.rs` (counting global allocator) |
| **Streaming is bounded** | `--stream` peak = largest paragraph + fixed caches, not the document | `alloctest.rs::streaming_peak_stays_flat_while_buffered_grows` |
| **Safe by default** | Opt-in features emit byte-identical output when off | `notes.rs`, `section.rs` module docs |
| **Byte-lossless save path** | Open+save without edits round-trips the original bytes (supported encodings) | `encode.rs` round-trip tests |
| **Apparatus integrity** | Broken markers are hard errors, not silent drops | `notes.rs` unit tests |
| **Real-document behaviour** | Frozen behaviour on the 8 example documents | `examples/README.md`, `reporttest.rs` |

The resource claim, from the instrument itself: the buffered path holds the
whole document and its markup live at once; `--stream` holds the largest
paragraph plus the engine's fixed caches (~2 MB: compiled pattern set + link
parser), *regardless of document length*.

### 2.2 The limits (stated, not hidden)

A scope document that lists only strengths is marketing. These are the edges.

1. **Word-level, not byte-level, preservation.** Property 1 proves every *word*
   survives; it does not prove whitespace, punctuation, or line structure
   survive exactly. Paragraph reflow is intentional and recorded as declared
   divergence in `tests/corpus/README.md`. Textrill is *content*-preserving,
   not *format*-preserving.
2. **No reverse path.** There is no HTML→text converter, so there is no formal
   round-trip losslessness proof and no `undo` from the output alone. The
   engine is one-way.
3. **`--lang` cannot reach templates.** Shipped templates hard-code
   `<html lang="en">`, so the option does not propagate into
   `--document_template`. A `{{textrill:lang}}` slot would fix it (see §7).
4. **Property-tested, not proved.** The properties are strong and enforced, but
   they are tests over generated and frozen inputs, not a proof-assistant
   artifact.
5. **Streaming is UTF-8-only.** `--stream` cannot run the full detection order
   (which needs every byte), so it refuses non-UTF-8 input rather than decode
   it differently from the buffered path.

---

## 3. Where it sits

*For: everyone; the comparison tables especially for evaluators.*

### 3.1 The pipeline

Where does text *move*, and where does Textrill carry weight?

```
   capture / OCR            the text of record           STRUCTURING            use
  ┌──────────────┐         ┌───────────────────┐      ┌─────────────┐     ┌──────────────────┐
  │ scan / photo │         │ plain text /      │      │  TEXTRILL   │     │ browser reading   │
  │ tesseract /  │ ──────► │ transcription     │ ───► │  plain text │ ──► │ citation by anchor│
  │ Transkribus  │         │ (durable, greppable│     │  → HTML     │     │ search / archive  │
  └──────────────┘         │  diffable, hashable)│     └─────────────┘     │ LLM / RAG ingest  │
        upstream           └───────────────────┘        the middle         └──────────────────┘
        (not us)               the format of record      (us)                 downstream
```

Textrill is the **middle**: after characters exist, before they are read or
consumed — not the capture stage, not the reading/publishing stage.

### 3.2 Neighbours

One line each: how it relates.

| Neighbour | Relationship |
|---|---|
| `txt2html` / `HTML::TextToHTML` | **Ancestor.** A port; the reference is a cross-check behind `make diff`, never the specification. |
| pandoc | **Markup-first.** Converts *between* markup formats; Textrill's input has no markup to begin with. |
| Markdown | A **lightweight markup language** — closer to the input side; Textrill consumes text never marked up at all. |
| TEI | A **scholarly encoding standard**; a plausible *downstream* exporter target, not a replacement. |
| OCR engines (tesseract, Transkribus) | **Upstream.** They produce the characters; Textrill structures them. Pairing: OCR → text → Textrill. |
| OCR post-processing / cleanup | **Adjacent.** Usually line-oriented fixes; Textrill gives layout and apparatus structural meaning. |
| EPUB / Calibre | **Downstream / parallel** publishing. EPUB is a delivery container; Textrill writes HTML. |
| Content-addressing / signing | **A pattern it could adopt** (hash the deterministic output), not a competitor. |
| LLM / RAG ingestion | **Downstream, incidental.** Labs' extraction/dedup/chunking stacks win at scale; Textrill's fit is *provenance, not ingestion*. See §3.3. |
| A generative "turn this into HTML" model | **The alternative to.** A model may invent or drop content; Textrill's contract is that it does not, and its failures are loud. |

*TODO: expand into a capability matrix — input format, requires markup?,
inference?, reversibility, determinism, memory bound, apparatus support,
licence.*

### 3.3 The LLM question

The LLM case deserves its own answer, because it is usually framed the wrong
way.

**Short version: as a general LLM-ingestion tool, no — and partly because the
arrow points the wrong way.** There is a narrow niche it fills, and it is not
the one "LLM case" usually means.

**Why the general case does not fit**

- **Direction mismatch.** LLM pipelines overwhelmingly want *structure
  stripped*: HTML/PDF → clean text. Textrill does the reverse (text → HTML) and
  does not parse HTML at all. It sits *upstream* of ingest, not in it.
- **The ingest niche is already better served.** Web/PDF → text: trafilatura,
  resiliparse, justext, readability, Unstructured, Marker, Nougat. Corpus
  building (FineWeb, Dolma, RedPajama) does dedup (MinHash/SimHash), quality
  filtering, decontamination, tokenizer-aware packing. Textrill loses this
  comparison.
- **Token economy is against HTML.** Tags cost tokens and lower information
  density in a fixed context; labs lean to plain text or Markdown. "HTML for
  LLMs" is a weak sell.
- **Labs optimize for other virtues.** Scale, quality, dedup, extraction
  robustness — not determinism, auditability, or byte-faithfulness. It is not
  that labs do Textrill's job better; Textrill's job is not on their scoreboard.

**Where it *is* genuinely useful — and labs don't compete**

- **Provenance/trust layer, not an ingest workhorse.** When you must show the
  text fed to a model is a *faithful* rendering of a source, a deterministic,
  content-preserving, offline converter with a stable hash is the right
  instrument. An audit/reproducibility need, not a pretraining need.
- **Citable anchors for a specific corpus.** Stable `#chunk-N` / `#note-*` ids
  let a model (or a person) cite a passage that resolves in a readable document.
  Nice, but not a differentiator against labs' source offsets + doc ids (§4.3).
- **Legacy plain-text corpora.** A scholar or small lab with
  convention-following text and no HTML gets structured, citable output cheaply
  and deterministically. Not a frontier lab.
- **A small, verifiable tool, not a research bet** — the opposite of what labs
  build and, in the archival/regulated corner, exactly what is wanted.

**Framing to defend:** Textrill is *not* an LLM tool — it is a
**trust/provenance layer that happens to emit machine-citable HTML**; any LLM
benefit is downstream and incidental. Positioning it "for LLMs" invites a
comparison it cannot win and hides the one it can (deterministic, auditable,
content-preserving conversion for scholarship and archives). If the goal really
were "help LLMs", the higher-leverage adjacent move is the *reverse* path
(HTML/PDF → text, or a clean Markdown/TEI exporter) — which walks into the
crowded ecosystem above.

### 3.4 Real documents

Tested against **real, unmanufactured documents** (no structure announced by
markup), recorded with provenance and licence in `examples/README.md`:

| Document | Kind | Exercises |
|---|---|---|
| Homer, *Odyssey* | long prose | paragraphs, link labels |
| Gelbenhügel | indie RPG setting, markdown-as-text | headings, links, NBSP/ZWSP |
| Septuagint (Swete, Genesis) | polytonic Greek | verse anchors, lacuna brackets, non-Latin script |
| *Mohe Zhiguan* 卷001 | classical Chinese | one continuous CJK paragraph (no blank lines), CJK runs |
| *Erya* 爾雅 | Chinese lexicon | terse entry/gloss lines → many `<br>` |
| Talmud Bavli, Sanhedrin 2a | Hebrew + Aramaic | Mishnah/Gemara/Rashi, cross-reference apparatus |
| Blake, *Songs of Innocence* | English poetry | short lines, headings |
| Apollinaire, *Calligrammes* | French concrete poetry | NBSP-run shapes; many fragments |

**Works well:** convention-following prose; multilingual and legacy encodings
(KOI8-R / CP1251 / CP1253, plus BOM → UTF-16 → UTF-8 → CP1252 detection);
verse, headings, lists; apparatus via namespaced markers; very large inputs via
`--stream`.

**Misfits:** page-image layout (no lines to infer from), richly tabular or
multi-column data, already-marked-up input, and any demand for *format* rather
than *content* preservation. Concrete-poetry shapes are lost in linear text
(see §4.4).

---

## 4. Using it well

*For: practitioners — scholars, archivists, engineers wiring it into a pipeline.*

### 4.1 Configuration: presets and templates

Options come from the command line, `@file` groups, and rc files; a preset is
just a named bundle of them.

The GUI's **named starting points** — `Plain`, `Article`, `Book`, `Man page`,
`Slides`, `Bare` (`textrill-gui-rs/src/presets.rs`) — each bundle a shipped
template library and, for templates carrying the slot, sectioning plus a table
of contents. Two properties matter:

- **A preset is not a mode.** It only *writes options*; everything it sets stays
  visible and editable, and a "Modified" marker appears the moment live options
  diverge. No hidden state.
- **Presets are pure data.** Documented option values compared against live
  options through the CLI's canonical spellings; nothing is special to the
  engine.

Today `PRESETS` is compiled in. The natural extension — and a distinct use case
— is **user-authored presets**: a saved option set for a document class,
essentially a **named, reusable rc fragment** that the GUI surfaces as a button.
Examples, all using the existing engine and no new conversion behaviour:

- Classicist: `lang=el`, `--lang_runs`, `--citations`, custom
  `--document_template` → "Greek critical edition".
- Archivist: KOI8-R/CP1251 input, `--meta_charset`, house stylesheet → "legacy
  Cyrillic transcript".
- Lecturer: the `slide` library, `--number_headings` → "my lecture deck".
- LLM-ingestion engineer: `--chunk` + stable anchors + `--extract` →
  "retrieval-ready segments".

It is coherent and cheap: no engine behaviour is added, output changes only when
selected (§2.1), and the precedence model (rc → `@file` → command line) already
exists. Open questions if it becomes real: storage and portability; whether
presets may reference user template files (they must, for the classicist);
whether a `--preset` flag joins the CLI or stays GUI-only; validation (the
"every preset is a valid bundle" test is the model); and precedence without
surprises.

This also reveals the shape of the product: the engine is a **set of composable
options**, and presets surface a curated subset. So "does it do X?" is best
answered in the option vocabulary, not as new modes.

#### Templates: bring your own

A template is just a file you point at by path; it replaces the page skeleton
the engine would otherwise write. Two levels:

- `--body_template FILE` wraps the converted **body**. The engine still writes
  the doctype, `<head>` and `<body>`, so you write only the fragment.
- `--document_template FILE` takes over the **whole page** — you write the
  doctype, head and body yourself.

Either file must contain `{{textrill:content}}`; a missing or malformed one is
reported before any output is written. It may place the other fixed slots —
`{{textrill:head}}`, `{{textrill:header}}`, `{{textrill:toc}}`,
`{{textrill:citations}}`, `{{textrill:glossary}}` — and named parameters
`{{textrill:var:name}}`, filled with `--var name=value` (verbatim, so you own the
escaping). Copy a shipped pattern from `textrill/templates/`:

```html
<!DOCTYPE html>
<html lang="en">
<head>
{{textrill:head}}
</head>
<body>
{{textrill:header}}
{{textrill:toc}}
<main>{{textrill:content}}</main>
</body>
</html>
```

```sh
textrill --document_template mypage.html --title "My Page" chapter.txt
```

Two things trip people up:

- **`{{textrill:head}}` already includes the `<title>`.** Adding your own
  `<title>{{textrill:title}}</title>` alongside it produces *two* titles. Use
  `{{textrill:title}}` on its own only when you hand-write `<head>` without the
  `head` slot.
- **The path is relative to where you run the tool**, not the input file. To use
  a template every time, put the option in `./.textrillrc` (it is an ordinary
  option value):
  ```
  --document_template templates/mine.html
  ```

In the GUI, the options panel's **Template source** radio offers *No template*,
*Body wrapper*, *Whole document* and *Shipped*; pick one and type the path (or
the shipped name). The three template options are mutually exclusive in both
the CLI and the GUI.

### 4.2 Apparatus: citations and glossary

Scholarly apparatus is explicit, never inferred. Namespaced markers in the
source text — `{{textrill:cite:key}}` and `{{textrill:gloss:key}}` — become real
endnotes and a definition list when `--citations` / `--glossary` are enabled;
off by default, they leave output byte-identical (§2.1). A dangling, duplicated,
or unbalanced marker is a **hard error**, not a silent drop. The generated
anchors (`note-ref-*`, `gloss-*`) are stable and deterministic. This is what
lets annotations, citations and glosses live *in the text of record* without a
markup language.

### 4.3 Citable anchors and provenance

Textrill's anchors sit beside the identifiers a corpus already uses. Both are
**locators, not descriptions** — like a call number, they say *where*, not
*what*.

- **Doc ID.** An identifier for one whole document in a corpus or index: a GUID,
  URL, filename, accession number, or content hash. Its job is to retrieve,
  deduplicate, attribute. `doc_id = 7c9a…` is a pointer carrying no meaning. (A
  content hash is the special case: it *does* change with the content but still
  tells you nothing about what the content is.)
- **Source offsets (spans / byte ranges).** Positions marking the exact text a
  chunk came from, e.g. `{"doc": "7c9a…", "start": 4096, "end": 4187}`. They
  point back to the precise passage for attribution, dedup, evaluation, and
  "which source did the model see?". Also opaque: location, not meaning.
- **Why labs like them.** Stable, cheap, unambiguous, and they let you
  reconstruct exactly what was fed to a model. They never need to be
  human-meaningful.

Textrill's `#chunk-7` is *also* just a locator; it says nothing about content
either. The differences: (a) the target is a **human-readable, portable
rendered document**, so a citation resolves to something a person can open and
read, not a private index; (b) some anchors are **author-meaningful** —
`#note-knuth` / `#gloss-monad` come from the author's own key, so the id is a
sliver of content, not an opaque number; and (c) they are **deterministic from
conventions**, reproducible by anyone re-running the converter rather than
minted by a database. The win is not "better metadata" but a *citation target
that is open, stable and reproducible*. The two approaches compose: store the
doc hash and the anchor together.

### 4.4 Concrete poetry and `<pre>` (work in progress)

> Status: **investigation only; no code written.** A direction, not a spec;
> nothing here is committed.

Concrete poetry puts meaning in the **spatial shape** of the page. Can Textrill
retain that shape using `<pre>`? Today `examples/README.md` notes that
Calligrammes' "concrete-poetry shapes [lost] in linear text".

**What is already true (verified).** The raw material is not destroyed; it is
simply not always boxed.

- **Explicit markers work.** With `--use_preformat_marker` (`<PRE>` … `</PRE>`
  are the default), the block is emitted as `<pre class='quote_explicit'>` and
  the body passes through **byte-for-byte**; the NBSP bytes survive verbatim
  (`U+00A0` = `C2 A0`). The shape renders in `<pre>`'s default monospace font.
- **ASCII-space shapes auto-detect.** `preformat_trigger_lines = 2`,
  `preformat_whitespace_min = 5` (`options.rs:409-413`): two or more consecutive
  lines with ≥5 leading spaces are wrapped in `<pre>`. Predicate
  `is_preformatted` (`convert.rs:1772`, `\s{n,}\S+` or `\.{n,}\S+`).
- **NBSP shapes do *not* auto-detect.** `\s` expands to the ASCII class
  `[ \t\r\n\x0c\x0b]` for Perl parity (`links.rs:85`), excluding `U+00A0`.
  Calligrammes uses 3,461 NBSP runs, so it slips past detection. Those runs
  still pass into `<p>`, where HTML does not collapse NBSP, so the shape
  *partly* survives already — just not in a monospace box.

**The safest route (proposed, not built).** *Stay explicit; do not infer.*
Concrete poetry is not code, and the principle is *no inference, fail loud
rather than guess*. So: a **dedicated, opt-in mark** rather than an automatic
heuristic — a marker pair (e.g. `<CAL>` … `</CAL>`), off by default, reusing the
existing preformat machinery, emitting `<pre class="calligramme">` (or similar)
as a styling hook. (`quote_explicit` is a semantic misnomer here.) Opt-in and
explicit, it cannot false-positive on indented prose and cannot change existing
output — consistent with §2.1.

**The route we are deferring.** An **automatic NBSP-aware shape detector** (add
NBSP to the whitespace class, or a whitespace-density heuristic). More
convenient, but it is the one place the tool would *guess*, risking `<pre>`
around ordinary indented prose. If pursued: conservative, opt-in, and measured
for false positives against `examples/` and the corpus before acceptance.

**Open questions before building:**

- Marker syntax and class name (`calligramme`? `concrete`? reuse
  `quote_explicit`?).
- Keep NBSP verbatim (maximally faithful, current) or normalise to ASCII inside
  `<pre>` (cleaner)? Either is deterministic.
- Accessibility: `<pre>` keeps the text readable linearly, but a shape is a
  stream of spaces to a screen reader. `role="img"` + `aria-label` helps sighted
  users but removes the text from the accessibility tree — decide deliberately.
- Which example(s) become the acceptance fixture, and what golden shape is
  approved? (Any output change means a golden recapture; see CONTRIBUTING.)

**Tentative verdict:** an **explicit, opt-in `<pre>` marker** is *in scope and
aligned* — deterministic, no inference, no output change unless asked. Automatic
detection is *deferred*; it would put a guess on the conversion path, which the
rest of the design deliberately avoids.

### 4.5 A worked pipeline: PDF / web → accessible HTML

The case that best shows Textrill in place: take a hostile, badly-laid-out,
inaccessible PDF; extract its text; re-emit it as a clean, semantic,
WCAG-friendly HTML edition. Textrill is the last, most trust-worthy step; the
extraction tooling does the messy part.

#### 4.5.1 Extraction: getting clean text out

**PDF → text** (prefer non-executing extractors; test `-layout` vs `-raw`):

| Tool | Use |
|---|---|
| `pdftotext -layout` / `-raw` (poppler) | Fast baseline; `-layout` keeps columns, `-raw` keeps reading order. |
| `mutool draw -F text` (MuPDF) | Alternative reading-order extraction. |
| `pdftohtml -xml` (poppler) | Positioned text with structure, for programmatic cleanup. |
| `ocrmypdf --deskew --clean` + Tesseract | **Scanned** PDFs: adds a text layer. |
| Tesseract directly on page images | When there is no text layer at all. |
| Docling / Marker / Nougat | ML layout understanding → Markdown; better on columns/tables, heavier. |
| GROBID | Scholarly PDFs → TEI XML (headings, references, abstract). |
| PyMuPDF / `pdfplumber` / pdfminer.six | Programmatic block/table access; good for scripted pipelines. |

**Web / HTML → text** (extract *main content*, drop boilerplate):

| Tool | Use |
|---|---|
| trafilatura | Best general main-content + metadata extraction. |
| resiliparse | Fast, built for corpus-scale extraction. |
| readability-lxml / `@mozilla/readability` | Article extraction. |
| justext | Language-aware boilerplate removal. |
| pandoc `-f html -t plain` / `-t markdown` | Straight conversion when the HTML is already clean. |
| `lynx -dump` / `w3m -dump` | Quick-and-dirty; noisy — prefer trafilatura. |

#### 4.5.2 Preparing the text for Textrill

Textrill infers structure from **conventions**, so feed it conventions:

1. **UTF-8, one blank line between paragraphs.** The paragraph model is built on
   blank lines; a wall of text becomes a wall of text.
2. **Fix extraction artifacts before, not after.** Join lines broken mid-word;
   `--unhyphenation` handles soft hyphenation, but de-hyphenate during
   extraction too. Strip running heads, page numbers, footers, duplicated
   headers.
3. **Recover the reading order.** Multi-column PDFs are the usual failure; use a
   block-aware extractor (Docling/PyMuPDF/`pdftohtml -xml`) rather than
   `pdftotext` when columns are present.
4. **Mark structure explicitly where layout is unreliable.** Setext underlines
   for headings, indentation for lists, the `{{textrill:cite:…}}` /
   `{{textrill:gloss:…}}` markers (with `--citations` / `--glossary`) for
   apparatus. Explicit marks are how a visual-format document becomes a semantic
   one.
5. **Verify what was recovered.** `--report` prints headings / paragraphs /
   strong / br counts; a drop in paragraph count is a red flag before you ever
   look at the HTML.
6. **Scale.** `--stream` for large inputs (UTF-8 only); `--chunk` for one file
   per section.

#### 4.5.3 Why this improves accessibility (WCAG)

PDFs are notoriously inaccessible: tagged PDFs are rare, reading order is
frequently broken, fixed pages do not reflow. Semantic HTML fixes the common
failures:

- **1.4.10 Reflow** — no horizontal scrolling at 320 px; HTML flows, a fixed PDF
  page does not.
- **1.4.4 Resize text / 1.4.12 Text spacing** — the user can zoom and space text;
  a PDF's layout resists both.
- **1.3.1 Info and relationships** — layout inference yields real `<h1>`–`<h6>`,
  `<ul>/<ol>`, `<dl>` and paragraphs that assistive tech can navigate.
- **3.1.1 Language of page** — `--lang` declares the language for screen readers
  (and, once §2.2's gap is closed, reaches `--document_template`).
- **2.4.1 Bypass blocks** — templates can supply `<main>` / `<nav>` landmarks.
- **No script, no tracking** — the output contains no JavaScript and fetches
  nothing; `--no_referrer` also avoids leaking referrers.

The GUI's own display controls (Light/Dark/Sepia/High-contrast held to WCAG AAA,
text-size stepping, reduce-motion, focus mode) are the same commitment applied
to the app itself.

#### 4.5.4 Security posture for hostile PDFs

PDF is an **active, hostile format**: JavaScript, launch actions, embedded files,
a long history of parser exploits. Treat extraction as untrusted input handling:

- **Do not open an untrusted PDF in a full-featured viewer.** Use parsers, not
  runtimes: `pdftotext`, `mutool`, `ocrmypdf`, Tesseract.
- **Sandbox the conversion** — a container, no network, dropped capabilities, or
  bubblewrap/Firejail. Even non-executing parsers have memory-safety bugs.
- **Strip metadata and attachments** (`exiftool`, or simply never carry them
  forward).
- **The final artifact is inert.** Textrill emits no `<script>`, and
  `--allowed_url_schemes` refuses script-bearing schemes, so a malicious link
  cannot become a live `javascript:` href.
- **Hash the stages.** Deterministic conversion lets you hash both source and
  output, so the transformation is auditable end to end.

#### 4.5.5 What the pipeline shows

Textrill is the **structuring and trust step**: after characters exist, before
anyone reads or ingests them. It does *not* do extraction, OCR, layout analysis,
or de-boilerplating — those are upstream, and §4.5.1's tools are better at them.
Its contribution is that the last step is **deterministic, content-preserving,
offline, and auditable**, turning "some text a script scraped" into "an edition
you can cite, verify, and feed to a human or a machine".

---

## 5. How we know it works

*For: engineers, reviewers, funders. This section turns claims into numbers.*

### 5.1 One conversion (micro yardsticks)

| Question | Metric | How |
|---|---|---|
| Did we lose text? | content-preservation rate over generated inputs | `make proptest` |
| Is the output valid? | well-formedness on generated inputs | `make proptest` |
| Is it stable? | output hash across runs | run twice, compare (goldens already pin bytes) |
| How much memory? | peak live bytes, buffered vs `--stream` | `make alloctest` |
| What did it recover? | bytes / headings / paragraphs / strong / br | `textrill --report` |
| Does it match approved behaviour? | byte-identity to goldens | `make accept` |

### 5.2 A corpus or a deployment (macro yardsticks)

- **Coverage:** 63 corpus cases, 32 upstream author goldens, 8 real examples —
  all byte-pinned. A run prints `SELF: 63/63`, `AUTHOR: 32/32`.
- **Cost vs alternatives:** one deterministic CPU pass; ~2 MB fixed working
  set; output in kilobytes vs page images in megabytes vs token budgets in LLM
  context windows. *(To become a measured table once run over a known corpus.)*
- **Failure visibility:** every structural inference is reportable (`--report`)
  and every apparatus mistake is a hard error, so "it worked" is falsifiable
  per document, not a matter of trust.

### 5.3 Scorecard (audience → answer)

| Audience asks… | Answered by |
|---|---|
| "Will it lose my text?" | content-preservation property (§2.1) |
| "Can I trust the machine output?" | determinism + well-formedness + anchors (§2.1) |
| "Will it fit my machine / archive?" | streaming peak + linear allocation (§5.1) |
| "Does it handle my material?" | example corpus + field notes (§3.4) |
| "What does it cost?" | compute/memory/storage comparison (§5.2) |
| "How is this different from an LLM?" | the guarantees; a model has none (§3.3) |

---

## 6. When to use it

*For: everyone; this is the short answer to "should I use this?"*

### 6.1 What it is genuinely good for

Usefulness is **narrow but deep**. It is good where all of these hold:

1. The input is **plain text following author conventions** (blank lines,
   indentation, setext underlines), not a markup format.
2. You want **deterministic, content-preserving HTML** and care that no words
   vanish silently.
3. **Format reflow is acceptable** — the words survive, the exact line breaks
   and spacing need not.
4. You value **no inference and loud failure** over convenience — e.g.
   apparatus that must error rather than guess.
5. It can run **offline on modest hardware**, one paragraph at a time.

Concrete coherent uses, then:

- **Digitization post-processing:** after OCR/transcription, before reading and
  publishing — the structuring step in a text pipeline.
- **Trustworthy editions:** stable `id` anchors, endnotes and glossary with no
  JavaScript, citable by anchor.
- **Machine ingestion:** deterministic HTML with stable anchors as a safe front
  door for LLM/RAG retrieval.
- **Large corpora:** `--stream` converts a multi-gigabyte archive without ever
  holding it whole.
- **Embedding:** use the crate as a library inside a larger pipeline.
- **Reproducibility:** with a hash wrapper, a content-addressed record that a
  given edition derives from a given source.
- **As a reference implementation** of an auditable, test-gated transformer.

### 6.2 What not to do with it (anti-patterns)

Each is a mismatch, not a bug. Most follow from the non-goals (§1.4); the
distinct ones are marked.

- **Not OCR.** It does not read pixels; feed it characters.
- **Not format-preserving.** No original whitespace, line breaks, or column
  positions come back intact (§2.2).
- **Not a markup authoring tool** or WYSIWYG editor — the input has no tags.
- **Not a reverse/parse tool.** No HTML→text path (§2.2).
- **Not for complex layout** — multi-column pages, rich tables, embedded media,
  arbitrary CSS are out of scope.
- **Not a generative rewriter.** It never invents or "improves" content; it
  refuses broken apparatus rather than guessing. Wrong tool for paraphrase.
- **Not a shape-inference engine.** Visually-arranged text needs an explicit
  mark (§4.4).
- **Not a spec reproduction.** The Perl reference is a cross-check, not the
  truth; where they differ, textrill decides.
- **Not a packaged product yet.** No Flatpak/distro release is published
  (`packaging/README.md`).

### 6.3 The coherence test

A proposed use is **in scope** iff it: starts from convention-following plain
text; wants deterministic, content-preserving HTML; tolerates format reflow;
prefers no-inference and loud failure; and runs offline on CPU.

It is **out of scope** if it needs (i) byte/format fidelity, (ii) structure that
is not conventionally present and must be inferred, or (iii) a model in the
loop.

### 6.4 Verdict

Many tools "turn text into HTML". Textrill's value is not that sentence; it is
the qualifier: **deterministically, without dropping content, with the apparatus
intact, failing loudly, in bounded memory.** Its usefulness concentrates where
**trust, auditability and reproducibility matter more than features or
fidelity** — scholarly and archival pipelines, machine ingestion, and large
batch jobs. Inside that boundary it is a dependable instrument; used as a
general document converter or a generative tool, it will disappoint on purpose.

---

## 7. Open questions

*For: the room. Decisions still owed.*

1. **Reverse converter (HTML→text): in scope or permanent non-goal?** A reverse
   path would enable round-trip losslessness, diff/undo, and a stronger trust
   story — at the cost of a second, harder engine.
2. **How far do we push content preservation?** Aim at a
   whitespace/structure-preservation property, or state word-level as the
   honest limit?
3. **Should `--lang` reach templates?** A `{{textrill:lang}}` slot is a small,
   real fix; is it in the next cut?
4. **Do content-addressing and signing enter scope?** Hash and optionally sign
   the deterministic output so a rendered edition provably derives from a
   source. Strong for scholars and AI-trust; adds surface.
5. **New output targets?** TEI, EPUB, Markdown exporters sharing the
   deterministic core — how many is coherent rather than diluted?
6. **Who signs off on scope?** With this many audiences, the document needs an
   owner and a rule for what counts as in-scope.

---

## Glossary (for non-specialists)

- **Plain text / format of record:** the bare characters, no formatting markup.
- **Markup language:** text with tags (HTML, Markdown, TEI) that *declare*
  structure.
- **Layout inference:** guessing structure from how text *looks* (indentation,
  blank lines) rather than from tags.
- **Well-formed:** every open tag properly closed and nested (properly, not
  pretty).
- **Deterministic:** same input always gives the exact same output bytes.
- **Streaming:** process a bit at a time instead of loading everything into
  memory.
- **Content-addressable:** a value derived from a hash of the content, so it
  changes if the content does.

---

### Revision log

- v1 — first braindump. Structure fixed; numbers and comparisons to be filled.
- v1 (restructure) — tightened prose; merged the old evidence index into §2.1.
- v1 (resequence) — dropped the Part/addendum split and folded every addendum
  into the main sequence: the LLM case is §3.3, presets and the usefulness
  assessment are §4.1 / §6, provenance is §4.3, concrete poetry is §4.4, and the
  PDF pipeline is §4.5. No content removed.
- v1 (templates) — added a concrete "bring your own template" recipe to §4.1
  (the two levels, required `content` slot, `--var`, the duplicate-title gotcha,
  rc and GUI paths).
