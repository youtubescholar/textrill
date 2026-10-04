# Template / feature research findings

Context: a prior abandoned attempt at the same product (a parallel txt2html
port, "the swarm") was mined from a 352 MB scratch directory and then deleted.
This records only what is reusable or decision-relevant. The swarm's
self-imposed process apparatus (agent-residue policing, append-only
claim/evidence ledgers, machine-checked-claim ceremony) is deliberately not
recorded; its own build notes concluded that apparatus became the failure mode.

Nothing was copied from the swarm. Everything below is either re-derived here or
noted as a hint to re-derive.

## 1. Decisions taken from this

- **Sectioning + TOC will support both output models, behind flags.**
  - Single-page sectioned: wrap each heading-delimited section in
    `<article class="section" id="chunk-N">`, emit one generated TOC.
  - Multi-file chunked: split into separate pages with a prev/next pager.
- Section/TOC ids are **sequential `chunk-N`**, assigned by the sectioner,
  *not* the `make_anchors` `section_x_y` names. This sidesteps duplicate-heading
  and level-gap issues entirely; the TOC cannot contain a dead link by
  construction. (The plan's earlier note about reusing `make_anchors` ids is
  superseded by this.)
- New markers are **default-off** and use **collision-proof sigils** only. The
  `C#`/`F#` case below is the evidence: ordinary-character delimiters already
  mangle real text.

## 2. Reusable designs from the swarm

- **CSS-only citation/glossary reveal (zero JS).** Hidden
  `<input type=checkbox>` + `<label for=...>`; shown via
  `input:checked ~ .notes li.note-ref-k { display: block }`, with a
  `:focus-visible` variant. Definitions are hard-errored when dangling,
  duplicate, empty or ambiguous; output is byte-identical to plain when the mode
  is off and unused. Directly transferable if we ever build notes.
- **`<details><summary>Contents</summary>…</details>`** as a no-JS collapsible
  TOC. Validates the no-JS constraint for a TOC control.
- **Flatpak finish-args** for the packaging phase:
  `--share=ipc --socket=x11 --socket=wayland --device=dri --filesystem=home
  --talk-name=org.freedesktop.portal.OpenURI`, runtime
  `org.freedesktop.Platform`/`SDK` 24.08. The swarm's GUI module was PySide6 via
  pip; ours is pure-Rust `egui`, so that module becomes the rust-stable SDK
  extension plus vendored `cargo-sources.json`. The finish-args still transfer.
- **Public-domain book pipeline contract**: prefer `*_djvu.txt` OCR, reject
  dark/empty items fail-closed, strict UTF-8, normalize whitespace, write a
  `PROVENANCE.md` with a sha256. Usable for the Wikisource/IA test-data plan.
- **No `href` is ever derived from document text.** Where that holds, CSP is a
  backstop and `javascript:` is structurally impossible. **We do not hold it**:
  `--make_links` and `--links_dictionaries` derive hrefs from document text, so
  autolinking needs scheme allowlisting and `rel="noopener noreferrer"` before
  it can be considered safe. The swarm's own note marks autolinking as the point
  where the guarantee ends. Recorded as a hazard, not yet acted on.

## 3. Hazards found in our own engine while checking the above

These are **inherited upstream defaults** (parity), not regressions. They matter
because they mean the tool is *not* a verbatim wrapper — it already rewrites
text by default. Evidence was produced with the release binary and the Perl
reference on 2026-10-04.

- **`demoronize` is on by default and rewrites typography** (`chars.rs:53-74`,
  applied at `convert.rs:2440`, default `true` at `options.rs:327`). The map:
  `…`→`...`, `'`→`` ` ``, `'`→`'`, `"`→`"`, `"`→`"`, `•`→`*`, `–`→`-`,
  `—`→`--`, `‚`→`,`, `„`→`,,`, `ˆ`→`^`, `‹`→`<`, `Œ`→`Oe`, `›`→`>`, `œ`→`oe`.
  Confirmed: `A—B` → `<p>A--B</p>`. Off via `--no_demoronize` (also forced off
  by `--eight_bit_clean`). This is the declared Tier-2 divergence: Perl emits
  bytes, we decode then demoronize, so rendered text agrees and bytes differ
  (`lib.rs:60-85`; `cp1252_smart` corpus case).
- **The default `#` bold delimiter eats real characters.** Confirmed:
  `I know C# and F# well.` → `<p>I know C<strong> and F</strong> well.</p>` —
  both `#` characters consumed, " and F" bolded. Matches the reference. Off via
  `--bold_delimiter ''` (or a different delimiter). A known UX hazard for
  technical prose.
- **Heuristic structure is imposed on plain lines**: ALL-CAPS → `<strong>`;
  `----` (4+) → `<hr/>` (dashes consumed); leading `- ` → `<li>` (marker
  consumed); a line ≤ `short_line_length` (40) gets a `<br/>` rather than being
  joined; setext underlines → headings; indentation → lists/pre. Escaping of
  `< > &` is on by default.

These are worth remembering before claiming the tool "does not touch the text".
Any new semantic feature should be *less* imposing than these defaults.

## 4. Not transferred, and why

- The swarm's structure-recognition ratchet, `floor.json`, and "blank-line
  segmentation is load-bearing" findings belong to a converter written from
  scratch to *recognise* structure. Ours is a byte-parity port with a
  differential corpus, where the Perl reference defines segmentation and the
  oracle is byte-comparison. Those gates do not apply.
- The swarm's chunk TOC used first-paragraph snippets because its converter was
  not heading-aware. Ours is, so our TOC is heading-based.
- Its multi-file chunking by paragraph count (`chunk-limit 28000`) is not the
  model we chose; ours splits at headings.
