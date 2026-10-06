# corpus case definitions: EXTRA[stem] = perl txt2html args, CLI[stem] = rust flags
declare -A INPUT
declare -A CTOR
declare -A EXTRA CLI
declare -A DICT
# GOLDEN[stem] overrides the tfiles/good_<stem>.html default, for the case where
# upstream scores several conversions against one golden. See empty1.
# shellcheck disable=SC2034  # consumed by the sourcing run.sh, like the arrays above
declare -A GOLDEN
# Cases whose stem has a tfiles/good_<stem>.html golden but which legitimately
# cannot match it, with the reason.  Empty means every case with a golden must
# match it byte for byte.
declare -A NOGOLDEN

NOGOLDEN[custom-headers2]='golden records section_3..5: upstream t/20tfiles.t reuses one $conv for all cases and never resets the heading-anchor counter, so custom-headers consumed 1..2. This runner builds a fresh converter per case, which is what a fresh Perl run does, and the port matches that exactly'
NOGOLDEN[pre2]='golden is 141 bytes, converter output is 140: the only difference is a trailing newline the converter never emits. Upstream compare() strips CR/LF per line and cannot see it'
# The heading patterns must match t/20tfiles.t:87 exactly --
# ['^\d+\. +\w+', '^\d+\.\d+. +\w+', '^\d+\.\d+\.\d+. +\w+'].  An earlier
# version of this file wrote the 2nd and 3rd as '\d+\.\d+ +\.\w+', which needs
# a literal dot after the space and so never matches a heading like "1.1.  SCOPE";
# the case still passed because the same wrong pattern went to both sides, and it
# emitted <strong> where upstream emits <h2>.
EXTRA[custom-headers]='custom_heading_regexp=>['"'"'^\\d+\\. +\\w+'"'"','"'"'^\\d+\\.\\d+. +\\w+'"'"','"'"'^\\d+\\.\\d+\\.\\d+. +\\w+'"'"'],extract=>1'
CLI[custom-headers]='--custom_heading_regexp "^\\d+\\. +\\w+" --custom_heading_regexp "^\\d+\\.\\d+. +\\w+" --custom_heading_regexp "^\\d+\\.\\d+\\.\\d+. +\\w+" --extract'

EXTRA[custom-headers2]='custom_heading_regexp=>['"'"'^What: '"'"'],extract=>1'
CLI[custom-headers2]='--custom_heading_regexp "^What: " --extract'

EXTRA[hyphens]='extract=>1'
CLI[hyphens]='--extract'

EXTRA[links]='extract=>1'
CLI[links]='--extract'

EXTRA[links2]='extract=>1'
CLI[links2]='--extract'

EXTRA[list]='extract=>1'
CLI[list]='--extract'

EXTRA[list-2]='extract=>1'
CLI[list-2]='--extract'

EXTRA[list-3]='extract=>1,xhtml=>1'
CLI[list-3]='--extract --xhtml'

EXTRA[list-4]='extract=>0,xhtml=>1'
CLI[list-4]='--xhtml'

EXTRA[list-5]='extract=>0,xhtml=>1'
CLI[list-5]='--xhtml'

EXTRA[list-custom]="bullets=>'-=o+*',bullets_ordered=>'#',extract=>0,xhtml=>1"
CLI[list-custom]='--bullets "-=o+*" --bullets_ordered "#" --xhtml'

EXTRA[list-advanced]='extract=>0,xhtml=>1'
CLI[list-advanced]='--xhtml'

EXTRA[list-styles]='extract=>0,xhtml=>1'
CLI[list-styles]='--xhtml'

EXTRA[news]='extract=>0,mailmode=>1'
CLI[news]='--mailmode'

EXTRA[pre]='extract=>1,use_preformat_marker=>1'
CLI[pre]='--extract --use_preformat_marker'

EXTRA[pre2]='extract=>1,use_preformat_marker=>0'
CLI[pre2]='--extract --no-use_preformat_marker'

EXTRA[table-align]='extract=>1,make_tables=>1'
CLI[table-align]='--extract --make_tables'

EXTRA[table-pgsql]='extract=>1,make_tables=>1'
CLI[table-pgsql]='--extract --make_tables'

EXTRA[table-pgsql2]='extract=>1,make_tables=>1,xhtml=>1'
CLI[table-pgsql2]='--extract --make_tables --xhtml'

EXTRA[table-border]='extract=>1,make_tables=>1'
CLI[table-border]='--extract --make_tables'

EXTRA[table-delim]='extract=>1,make_tables=>1,xhtml=>1'
CLI[table-delim]='--extract --make_tables --xhtml'

# t/20tfiles.t:554,579,604,629 converts tfiles/empty.txt four times, once per
# extract/xhtml combination, and compares *all four* against the same golden,
# tfiles/good_empty.html.
#
# The stems here are upstream's *output* filenames (empty1.html .. empty4.html).
# Left at the default, INPUT[stem] is "$stem.txt" and run_case looks for
# tfiles/empty1.txt, which does not exist -- so all four cases were reading a
# missing file, and the reference and the port each produced 0 bytes and matched
# each other. They have been passing without testing anything since the corpus
# was imported. A9 is what exposed it: an unreadable input now exits non-zero,
# so the four cases fail loudly instead of comparing two empty files.
EXTRA[empty1]='extract=>0,xhtml=>0'
CLI[empty1]='--no-xhtml'
INPUT[empty1]='empty.txt'
GOLDEN[empty1]='good_empty.html'

EXTRA[empty2]='extract=>0,xhtml=>1'
CLI[empty2]='--xhtml'
INPUT[empty2]='empty.txt'
GOLDEN[empty2]='good_empty.html'

EXTRA[empty3]='extract=>1,xhtml=>0'
CLI[empty3]='--extract --no-xhtml'
INPUT[empty3]='empty.txt'
GOLDEN[empty3]='good_empty.html'

EXTRA[empty4]='extract=>1,xhtml=>1'
CLI[empty4]='--extract --xhtml'
INPUT[empty4]='empty.txt'
GOLDEN[empty4]='good_empty.html'

# system_link_dict is deliberately absent from CLI[] here: 3.0 removed the
# option from scripts/txt2html (ChangeLog: "no longer a --system_link_dict
# option") and neither the module nor the script mentions it any more, so
# Perl's args() simply swallows the key.  It is kept in EXTRA[] only to mirror
# t/20tfiles.t, which still passes it to the module.  Passing it on the Rust
# side made the binary exit 1 with "Unknown option `system_link_dict`".
CTOR[sample]='xhtml=>0'
EXTRA[sample]='system_link_dict=>"txt2html.dict",titlefirst=>1,mailmode=>1,custom_heading_regexp=>['"'"'^ *--[\\w\\s]+-- *$'"'"'],make_tables=>1,append_file=>"tfiles/sample.foot"'
CLI[sample]='--no-xhtml --titlefirst --mailmode --custom_heading_regexp "^ *--[\\w\\s]+-- *$" --make_tables --append_file "tfiles/sample.foot"'

EXTRA[xhtml_sample]='system_link_dict=>"txt2html.dict",titlefirst=>1,mailmode=>1,custom_heading_regexp=>['"'"'^ *--[\\w\\s]+-- *$'"'"'],make_tables=>1,make_anchors=>1,xhtml=>1,append_file=>"tfiles/sample.foot2"'
CLI[xhtml_sample]='--xhtml --titlefirst --mailmode --custom_heading_regexp "^ *--[\\w\\s]+-- *$" --make_tables --make_anchors --append_file "tfiles/sample.foot2"'

EXTRA[robo]='make_tables=>1'
CLI[robo]='--make_tables'

EXTRA[mixed]='make_tables=>0'
CLI[mixed]='--no-make_tables'

EXTRA[heading1]='make_tables=>0'
CLI[heading1]='--no-make_tables'

EXTRA[links3]='make_tables=>0'
CLI[links3]='--no-make_tables'

EXTRA[umlauttest]='make_tables=>0,xhtml=>1'
CLI[umlauttest]='--no-make_tables --xhtml'

EXTRA[utf8]='make_anchors=>0,extract=>1,eight_bit_clean=>1,xhtml=>1'
CLI[utf8]='--no-make_anchors --extract --eight_bit_clean --xhtml'

EXTRA[punct]='make_anchors=>0,extract=>1,eight_bit_clean=>1'
CLI[punct]='--no-make_anchors --extract --eight_bit_clean'

EXTRA[links4]='make_anchors=>0,extract=>1,eight_bit_clean=>1'
CLI[links4]='--no-make_anchors --extract --eight_bit_clean'
INPUT[xhtml_sample]=sample.txt

# shared converter state across two input files (heading anchor counters continue)
EXTRA[multi-file]='extract=>1'
CLI[multi-file]='--extract'
INPUT[multi-file]=custom-headers.txt,custom-headers2.txt

# --- cases taken from the upstream 3.0 CI smoke tests ---------------------
# .github/workflows/tests.yml is the only test 3.0 added over 2.51, and it pins
# the --links_dictionaries fix.  These use the upstream fixture files.
EXTRA[ci_simple]='extract=>1'
CLI[ci_simple]='--extract'
INPUT[ci_simple]=../.github/workflows/test1.txt

EXTRA[ci_links]='extract=>1'
CLI[ci_links]='--extract'
INPUT[ci_links]=../.github/workflows/test2.txt

EXTRA[ci_dict]='extract=>1'
CLI[ci_dict]='--extract --links_dictionaries .github/workflows/xyz.dict'
INPUT[ci_dict]=../.github/workflows/test3.txt
DICT[ci_dict]=.github/workflows/xyz.dict

# A URL that contains a word the system dictionary also links.  Perl guards the
# substitution with in_link_context over the text emitted so far; without that
# the inner word is linked again and the output nests <a> inside <a href>.
EXTRA[link_in_url]='extract=>1'
CLI[link_in_url]='--extract'
INPUT[link_in_url]=../.github/workflows/test2.txt

# --- regressions pinned by the P3 fuzzer ---------------------------------
#
# Each of these was a real byte-level divergence found by fuzz.py, kept as a
# fixed case so a future refactor cannot quietly reintroduce it.  The inputs
# live in tests/corpus/inputs/ rather than the reference's tfiles/.
#
# --table_type replaces the whole table-type set, it does not merge into the
# defaults (Getopt::Long n%).  With DELIM=0 having disabled DELIM, and ALIGN
# and PGSQL not named, the +-+-+ drawing must stay a paragraph, not become a
# BORDER table.
EXTRA[table_type_replace]='extract=>1,make_tables=>1,table_type=>{DELIM=>0}'
CLI[table_type_replace]='--extract --make_tables --table_type DELIM=0'
INPUT[table_type_replace]="$HERE/inputs/table_type.txt"

# And the positive control: naming BORDER=1 still builds a border table, so
# the test above is not passing merely because table detection was switched
# off wholesale.
EXTRA[table_type_named]='extract=>1,make_tables=>1,table_type=>{BORDER=>1}'
CLI[table_type_named]='--extract --make_tables --table_type BORDER=1'
INPUT[table_type_named]="$HERE/inputs/border_table.txt"

# An explicit-quote PRE block swallowed everything after its first blank line:
# the continuation text is buffered by split_end_explicit_preformat and only
# joined back inside the block that runs when text remains.
#
# The stem is `pre_explicit`, matching the input. It was `pre_explicit_blank`
# until A8 added a second, different case under that same stem later in this
# file, and bash's associative arrays take the last write -- so A8 silently
# overwrote this one. It was not red for a year: both variants are byte-identical
# to the reference, so the corpus still reported 47/47 with the case simply never
# running. Renaming is the fix; `duplicate_key_check` in run.sh is the guard that
# makes the next one loud. See that function for why the count could not catch it.
EXTRA[pre_explicit]='extract=>1,use_preformat_marker=>1'
CLI[pre_explicit]='--extract --use_preformat_marker'
INPUT[pre_explicit]="$HERE/inputs/pre_explicit.txt"

# --- A1: a paragraph far larger than any real-world use of the tool -------
#
# A single ~1 MB paragraph (no blank lines) used to abort the engine with
# fancy-regex's `BacktrackLimitExceeded`, and once the `(?=\n?$)` chop regex
# was replaced, to hang instead: the `\B` and `(?<!delim)` delimiter
# patterns in do_delim put fancy-regex on its backtracking VM, which
# explodes on paragraphs of roughly half a megabyte.  Perl renders both of
# these in about a second, so the port must as well.
EXTRA[huge_paragraph]='extract=>1'
CLI[huge_paragraph]='--extract'
INPUT[huge_paragraph]="$HERE/inputs/big_para.txt"

# Same scale, CRLF line endings, to keep the CR-chopping path in the test.
EXTRA[huge_paragraph_crlf]='extract=>1'
CLI[huge_paragraph_crlf]='--extract'
INPUT[huge_paragraph_crlf]="$HERE/inputs/big_para_crlf.txt"

# --- A1: rejected delimiter candidate must be retried, not swallowed -------
#
# The first `#` pair here spans a `</p><p>` boundary, so the bold pattern's
# assertion rejects it -- but the pair nested inside it still has to be
# picked up on the retry, which is what the original regex does when its
# lookahead fails. Found by fuzz.py (seed 99); the buffer also contains a
# link, which is what selects the in-link-context substitution path.
EXTRA[delim_retry]='table_type=>{ALIGN=>0},preformat_trigger_lines=>2'
CLI[delim_retry]='--table_type ALIGN=0 --preformat_trigger_lines 2'
INPUT[delim_retry]="$HERE/inputs/delim_retry.txt"

# --- explicit <pre> spanning a blank line, end marker in its own para ------
#
# Found by fuzz.py (seed 90210, case 30).  The end marker here is in a
# *different* paragraph from the text it closes, because of the blank line, and
# that is what routes the input through `split_end_explicit_preformat` instead
# of the per-line `endpreformat`.  In that function the reference's end-marker
# test at TextToHTML.pm:3868 is `if (${para_ref} =~ ...)` -- a symbolic
# reference, missing its `$`, to a global nothing ever assigns -- so the test
# never matches and the whole paragraph is emitted as preformatted text with the
# marker escaped into it.  PRE_EXPLICIT is not cleared either, so the trailing
# text stays inside the block.  The port now reproduces that; before the fix it
# dropped the escaped `&lt;/pre&gt;` line and moved `after the block` out of the
# block.  See the comment on `Converter::split_end_explicit_preformat`.
#
# No golden: upstream ships no `good_pre_explicit_blank.html`, so the
# differential comparison is the only oracle, and it is a strong one -- the
# point of the case is a byte difference the port used to get wrong.
EXTRA[pre_explicit_blank]='use_preformat_marker=>1'
CLI[pre_explicit_blank]='--use_preformat_marker'
INPUT[pre_explicit_blank]="$HERE/inputs/pre_explicit_blank.txt"

# --- A8: option values interpolated into the document are escaped ----------
#
# Two separate injections.  `--title` closes its own element and then opens a
# script element, because the value reached `<title>` unescaped; `--style_url`
# closes the href attribute the same way.  Neither is reachable from document
# text, so the fuzz corpus cannot produce them and they needed their own case.
#
# The port now escapes both (chars::escape_attr) and this is a Tier 2 divergence:
# the reference emits these bytes verbatim, so the differential comparison is
# expected to FAIL and is therefore not the oracle here.  The oracle is the
# second check below, which asks whether the output parses as XML -- before the
# fix it did not, and that is what the proptest suite had 30 known-open checks
# for.  See the A8 comment in fuzz.py's OPTION_DIVERGENT.
EXTRA[opt_injection]='title=>"</title><script>alert(3)</script>",style_url=>"x.css\" onload=\"alert(4)"'
CLI[opt_injection]='--title "plain" --style_url "plain.css"'
INPUT[opt_injection]="$HERE/inputs/pre_explicit_blank.txt"
NOGOLDEN[opt_injection]='differential must fail: deliberate Tier 2 divergence: the reference interpolates --title and --style_url into the document unescaped, which is a live XSS. The port escapes them, so a byte comparison against the reference must fail and cannot be the oracle; the oracle is the XML well-formedness check in proptest.py'

# --- A11: the URL scheme policy on generated hrefs ---
#
# A document can write a live `javascript:` or `data:` URL into its own output
# with a <URL:...> tag, and the reference does exactly that: the port's
# pre-A11 output for this input contained href="javascript:alert(document.domain)".
# The engine now refuses those schemes, unwraps the anchor and keeps the text.
#
# So this is the same shape as opt_injection above and for the same reason -- a
# deliberate Tier 2 divergence from a reference defect, which means the
# differential comparison CANNOT be the oracle. Saying so explicitly is the
# point of the "differential must fail:" prefix.
#
# The payload characters matter and are not arbitrary. The system dictionary has
# two <URL:...> rules; the first, <URL:foo:label>, only matches a label built
# from [a-zA-Z0-9'() ], and it splits the scheme off as the href. Put a '.', '/'
# or ';' in the label and that rule stops matching, the second rule takes the
# whole string as the href, and the scheme reaches the output. Every payload
# below carries one, so the case exercises the hole rather than the near miss.
# The last reference, <URL:javascript:alert(1)>, is the near miss and is in the
# input on purpose: the first rule matches it, the href is the relative word
# "javascript", and it must be left alone. Refusing it would be the policy
# pattern-matching a substring rather than reading a URL.
EXTRA[url_scheme]='extract=>1,make_links=>1'
CLI[url_scheme]='--extract --make_links'
INPUT[url_scheme]="$HERE/inputs/url_scheme.txt"
NOGOLDEN[url_scheme]='differential must fail: A11, a deliberate Tier 2 divergence where the port is unambiguously better. The reference emits whatever scheme a <URL:...> tag names, so this input came out of it with a live href="javascript:alert(document.domain)" and a live data: URL. The port refuses every scheme outside --allowed_url_schemes, unwraps the anchor and keeps the text, so a byte comparison against the reference must fail and cannot be the oracle. The oracle is tests/urlschemetest.rs, which asserts no refused scheme survives, that the words are kept, and that the relative href the label spelling produces is left alone'

# --- P7.1: the 0x80-0x9F range, where the fallback decode used to be wrong ---
#
# read_any_file falls back to a single-byte decode when the bytes are not valid
# UTF-8.  It decoded as Latin-1, but chars::demoronize_char's table is keyed on
# the *CP1252* code points (U+201C and friends), so a Latin-1 decode of the same
# byte produced U+009C/U+0092/U+0093 -- C1 control characters that are not in
# the table.  Every substitution silently did nothing on exactly the files
# demoronize exists to serve, and the C1 controls were re-emitted as UTF-8 into
# the HTML, where they render as nothing.
#
# This case cannot be a differential PASS and is not a NOGOLDEN lie either: the
# difference is in the *content* the reference gets right by accident.  Perl
# emits the raw bytes and a browser guesses CP1252 for them, so the reference
# renders " and -- correctly.  The port now decodes CP1252, demoronize fires, and
# it emits the ASCII equivalents -- so the rendered text agrees while the bytes
# do not, and the byte comparison below is expected to fail.  That is the whole
# point of the case, and the oracle is encodingtest.rs's assertions on the
# decoded code points, which are checked by cargo test and cannot silently rot.
EXTRA[cp1252_smart]='make_tables=>0,xhtml=>1'
CLI[cp1252_smart]='--no-make_tables --xhtml'
INPUT[cp1252_smart]="$HERE/inputs/cp1252_smart.txt"
NOGOLDEN[cp1252_smart]='differential must fail: P7.1. The reference emits CP1252 bytes verbatim and relies on the browser guessing CP1252; the port decodes CP1252 and demoronize rewrites the smart punctuation to ASCII. Both render the same text, the bytes differ, so a byte comparison cannot be the oracle. The oracle is tests/encodingtest.rs, which asserts the decoded code points and that no C1 control survives into the output'

# --- P7.1: wide characters in an aligned table -----------------------------
#
# byte_slice cuts table cells by byte offset, and the offsets come from
# table_columns over the OR-ed column map, so the whole thing is only coherent
# when every row is the same *byte* length.  It is: this fixture is built with
# every row exactly 21 bytes, including the row holding a 3-byte CJK character.
#
# The port gets this right and the reference does not -- Perl mangles the cell
# into &aelig;...&not; because it demoronizes each byte of a multi-byte sequence
# separately.  A second deliberate Tier 2 divergence, and here the port is
# unambiguously the better one: the text survives intact.  Same shape as
# cp1252_smart above -- differential expected to fail, unit tests are the
# oracle.
EXTRA[cjk_table]='make_tables=>1,xhtml=>1'
CLI[cjk_table]='--make_tables --xhtml'
INPUT[cjk_table]="$HERE/inputs/cjk_table.txt"
NOGOLDEN[cjk_table]='differential must fail: P7.1. Perl demoronizes each byte of a multi-byte UTF-8 sequence independently and mangles the CJK cell into Latin-1 entities; the port decodes UTF-8 first and the text survives. Deliberate Tier 2 divergence where the port is better, so a byte comparison against the reference must fail and cannot be the oracle; the oracle is the port preserving the cell text, asserted in tests/encodingtest.rs'

# --- P7.4: input encodings the reference has no notion of --------------------
#
# Six fixtures, two shapes, and the reason they exist is that the *reference
# cannot be the oracle for any of them*.  Perl reads raw bytes and leaves the
# encoding question to the browser, so for a CP1251 or a UTF-16 file it emits
# the bytes unchanged.  Every case below therefore *must* differ from the
# reference, and all six are declared NOGOLDEN for the same reason: a byte
# comparison would fail for a difference that is not a defect.
#
# What is asserted instead is in tests/encodingtest.rs, where the decoded code
# points are pinned and, for the UTF-16 cases, the absence of the NUL characters
# that used to appear between every letter.  That oracle is a unit test and is
# run by cargo test, so it cannot rot the way a checked-in golden can.

# Cyrillic in CP1251, KOI8-R and Greek in CP1253, read with the default.
#
# These are ordinary differential PASSes and are deliberately not declared
# NOGOLDEN. Under the default the port and the reference are *equally wrong*:
# neither can detect these encodings, both guess CP1252, and both emit the same
# mojibake. Declaring them NOGOLDEN would be claiming a divergence that does not
# exist -- the runner rejects that, correctly, and it is worth being explicit
# that P7.4 did not change the default behaviour for non-Western text.
#
# The real divergence is in the `_named` cases below, where the user supplies the
# encoding the file actually is.
EXTRA[cp1251_cyrillic]='make_tables=>0,xhtml=>1'
CLI[cp1251_cyrillic]='--no-make_tables --xhtml'
INPUT[cp1251_cyrillic]="$HERE/inputs/cp1251_cyrillic.txt"

EXTRA[koi8r_cyrillic]='make_tables=>0,xhtml=>1'
CLI[koi8r_cyrillic]='--no-make_tables --xhtml'
INPUT[koi8r_cyrillic]="$HERE/inputs/koi8r_cyrillic.txt"

EXTRA[cp1253_greek]='make_tables=>0,xhtml=>1'
CLI[cp1253_greek]='--no-make_tables --xhtml'
INPUT[cp1253_greek]="$HERE/inputs/cp1253_greek.txt"

# The same three files, read with the encoding named. These *are* declared
# NOGOLDEN, because now the port decodes correctly and the reference still emits
# raw bytes: the delta is the feature, and the oracle is tests/encodingtest.rs
# asserting the recovered text rather than any comparison against Perl.
EXTRA[cp1251_named]='make_tables=>0,xhtml=>1'
CLI[cp1251_named]='--no-make_tables --xhtml --encoding cp1251'
INPUT[cp1251_named]="$HERE/inputs/cp1251_cyrillic.txt"
NOGOLDEN[cp1251_named]='differential must fail: P7.4. The same CP1251 file as cp1251_cyrillic, read with --encoding cp1251. The reference has no encoding option and emits the bytes for a browser to guess, so it cannot produce this output at all. Deliberate Tier 2 divergence where the port is unambiguously better: the Russian text survives. The oracle is tests/encodingtest.rs asserting the recovered text, not a byte comparison against a reference that cannot read the file'

EXTRA[koi8r_named]='make_tables=>0,xhtml=>1'
CLI[koi8r_named]='--no-make_tables --xhtml --encoding koi8-r'
INPUT[koi8r_named]="$HERE/inputs/koi8r_cyrillic.txt"
NOGOLDEN[koi8r_named]='differential must fail: P7.4. KOI8-R named explicitly. Note CP1251 and KOI8-R disagree about nearly every byte above 0x80, so naming the wrong one is not a small error -- which is why the default is unchanged rather than changed to a guess. Oracle is tests/encodingtest.rs'

EXTRA[cp1253_named]='make_tables=>0,xhtml=>1'
CLI[cp1253_named]='--no-make_tables --xhtml --encoding cp1253'
INPUT[cp1253_named]="$HERE/inputs/cp1253_greek.txt"
NOGOLDEN[cp1253_named]='differential must fail: P7.4. Greek in CP1253 named explicitly. CP1253 leaves 17 of its 128 high bytes undefined and each is read as the Latin-1 C1 control, the same total-and-lossless rule the CP1252 fallback uses. Oracle is tests/encodingtest.rs'

# UTF-16 with no BOM, in both endiannesses.  This is the case the whole P7.4
# exists for: every byte is below 0x80, so the file is *valid UTF-8*, and both
# implementations accepted it and emitted a NUL between every character.  The
# oracle is tests/encodingtest.rs asserting the decoded text and that no NUL
# survives.
EXTRA[utf16le_ascii]='make_tables=>0,xhtml=>1'
CLI[utf16le_ascii]='--no-make_tables --xhtml'
INPUT[utf16le_ascii]="$HERE/inputs/utf16le_ascii.txt"
NOGOLDEN[utf16le_ascii]='differential must fail: P7.4. BOM-less UTF-16LE of ASCII prose, which is also valid UTF-8. The reference reads raw bytes and the port used to do the same, so both emitted NUL characters between every letter; the port now detects the NUL alignment. A byte comparison cannot be the oracle because the reference has no UTF-16 concept at all -- the oracle is tests/encodingtest.rs'

EXTRA[utf16be_ascii]='make_tables=>0,xhtml=>1'
CLI[utf16be_ascii]='--no-make_tables --xhtml'
INPUT[utf16be_ascii]="$HERE/inputs/utf16be_ascii.txt"
NOGOLDEN[utf16be_ascii]='differential must fail: P7.4. BOM-less UTF-16BE, same class as utf16le_ascii with the NULs on the other alignment. Oracle is tests/encodingtest.rs'

# UTF-16 *with* a BOM.  Here the reference's behaviour is not a defect but a
# non-feature: FF FE is a guarantee, and the port honours it instead of treating
# it as an undecodable byte and falling back to CP1252, where those two bytes
# have no meaning at all.
EXTRA[utf16le_bom]='make_tables=>0,xhtml=>1'
CLI[utf16le_bom]='--no-make_tables --xhtml'
INPUT[utf16le_bom]="$HERE/inputs/utf16le_bom.txt"
NOGOLDEN[utf16le_bom]='differential must fail: P7.4. UTF-16LE with a byte-order mark and Cyrillic text. The port reads the mark as a declaration and consumes it; the reference passes FF FE through as bytes. Oracle is tests/encodingtest.rs asserting the text and that U+FEFF does not lead the output'
