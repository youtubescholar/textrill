# corpus case definitions: EXTRA[stem] = perl txt2html args, CLI[stem] = rust flags
declare -A INPUT
declare -A CTOR
declare -A EXTRA CLI
declare -A DICT
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

EXTRA[empty1]='extract=>0,xhtml=>0'
CLI[empty1]='--no-xhtml'

EXTRA[empty2]='extract=>0,xhtml=>1'
CLI[empty2]='--xhtml'

EXTRA[empty3]='extract=>1,xhtml=>0'
CLI[empty3]='--extract --no-xhtml'

EXTRA[empty4]='extract=>1,xhtml=>1'
CLI[empty4]='--extract --xhtml'

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
EXTRA[pre_explicit_blank]='extract=>1,use_preformat_marker=>1'
CLI[pre_explicit_blank]='--extract --use_preformat_marker'
INPUT[pre_explicit_blank]="$HERE/inputs/pre_explicit.txt"

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
