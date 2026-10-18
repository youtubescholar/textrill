# examples/ — provenance register

Each file here is real, unmanufactured text (no structure announced by markup),
used to exercise the layout engine against documents a person actually wrote.

License/edition/source were confirmed at acquisition time (2026-10-08). Capture
method: headline render of the cited Wikisource page; navigation chrome removed;
text itself unaltered.

| file | source / edition | licence (confirmed) | counts (B/H/P/S/br) | notes |
| --- | --- | --- | --- | --- |
| homer.txt | Project Gutenberg Odyssey (pre-existing) | PD (Gutenberg) | 38477 / 0 / 64 / 39 / 34 | provenance in commit `ab2bb04` |
| gelbenhuegel.txt | github.com/carpedavid/gelbenhugel, `src/content/docs/` (125 md files, concatenated) | CC0 1.0 Universal | 44484 / 16 / 191 / 1 / 334 | David Garrett / Amalara Game Studio; city setting for Shadowdark/Cairn; markdown kept as-is |
| septuagint_swete_genesis.txt | H.B. Swete, *The Old Testament in Greek* I (Cambridge UP, 4th ed. 1909/1930), Genesis only, via `subatomicglue/AncientGreekSources` | PD (1909–1930 ed.) | 378854 / 0 / 50 / 0 / 50 | polytonic Greek; verse anchors; `[…]` = edition's lacuna reconstructions |
| mohe_zhiguan_vol001.txt | zh.wikisource.org `摩訶止觀/卷001` (智顗, Sui dynasty, 6th c.) | PD (author died >100 y; pre-1931) | 48433 / 0 / 1 / 0 / 7 | CJK text is one continuous paragraph (no blank lines); Wikisource transcription layer CC-BY-SA |
| erya.txt | zh.wikisource.org `爾雅` (Warring States–Western Han lexicon, all 19 釋-X chapters) | PD (author died >100 y; pre-1931) | 51304 / 0 / 41 / 0 / 1028 | terse entry-and-gloss lines yield many `<br>`; Wikisource transcription layer CC-BY-SA |
| talmud.txt | he.wikisource.org `סנהדרין ב א` (Talmud Bavli, Sanhedrin 2a, Vilna ed.) | PD (Vilna text: Wikisource states no copyright) | 13601 / 0 / 78 / 0 / 3 | Mishnah + Gemara + Rashi; עין משפט cross-ref apparatus present |
| blake.txt | en.wikisource.org, Shepherd 1887 ed. of *Songs of Innocence* | PD (Blake d. 1827; ed. 1887) | 16903 / 0 / 466 / 26 / 10 | poems incl. "The Lamb" |
| calli.txt | fr.wikisource.org `Calligrammes/Texte entier` (Apollinaire, Mercure de France 1918) | PD (d. 1918; pre-1928) | 123429 / 0 / 2814 / 164 / 3 | concrete-poetry shapes lost in linear text; many short fragments |

B/H/P/S/br = bytes / headings / paragraphs / `<strong>` / `<br>`, as printed by
`--report` and pinned in `textrill/tests/reporttest.rs`.

Note: the Wikisource pages themselves are CC-BY-SA as a transcription layer over
the PD underlying text; cite the page permalink for anything more than test use.

### Capture notes (invisible Unicode)

The rendered-capture transcriptions carry what a plain copy of the page carries,
and it is part of the counted bytes. Calligrammes' concrete-poetry shapes are
encoded by the transcribers as non-breaking-space runs (U+00A0, 3 461
occurrences) plus the standard French no-break space before `! ? : ; »`; the
Blake page carries 20 zero-width spaces (U+200B, Wikisource `{{zwsp}}`
line-wrap aids); Gelbenhügel's author markdown uses U+00A0 around links (13) and
one trailing U+200B; the Talmud page has 2 U+00A0 in commentary spacing. None
were introduced by the capture itself.

### Rejected candidates (checked 2026-10-08)

| game | verdict | reason |
| --- | --- | --- |
| Searchers of the Unknown | rejected | freeware, explicit "not to be sold"; not open |
| Basic Fantasy RPG | rejected | CC-BY-SA |
| D&D 5.1 / 5.2 SRD | rejected | CC-BY |
| Tunnel Goons (Nate Treme / Highland Paranormal Society) | rejected | CC BY 4.0 per the author's clarification on the itch.io community thread; CC0/PD-only bar kept the corpus attribution-free |