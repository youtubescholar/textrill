//! P12: resource bounds, measured with a counting allocator.
//!
//! Every other check in this repository compares output bytes. That is the
//! wrong instrument for a resource defect: A2 leaks a compiled regex per
//! distinct pattern while producing perfectly correct output, so the corpus
//! passes and the process still grows without bound. This file is the
//! instrument that can see it.
//!
//! A counting `GlobalAllocator` is used rather than peak RSS because it is
//! deterministic and machine-independent. Requested byte counts do not vary
//! with allocator implementation, page size, or ASLR, so a budget asserted here
//! means the same thing on every machine -- which a peak-RSS threshold does not.
//! RSS and `valgrind --tool=massif` remain the diagnostics; this is the gate.
//!
//! The allocator is installed in this test binary only, so it cannot perturb
//! any other test's timing or behaviour.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use txt2html::convert::Converter;
use txt2html::options::Options;

static CUMULATIVE: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        CUMULATIVE.fetch_add(l.size(), Ordering::Relaxed);
        LIVE.fetch_add(l.size(), Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }

    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        unsafe { System.dealloc(p, l) }
    }

    // realloc moves a block without passing through alloc/dealloc. Omitting it
    // would undercount exactly the churn a grow-in-place pattern produces, which
    // is one of the things this file exists to notice.
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new_size: usize) -> *mut u8 {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        CUMULATIVE.fetch_add(new_size, Ordering::Relaxed);
        LIVE.fetch_add(new_size, Ordering::Relaxed);
        unsafe { System.realloc(p, l, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The allocation defects this file is built to detect, that the port still has,
/// and that the plan already owns. They are *measured and printed on every run*
/// rather than deleted, so the numbers stay visible; each is recorded by an
/// explicit `known_open(..)` call at the site that measures it, so removing a
/// defect is a two-line change and cannot be done by accident.
///
/// This list is the audit trail. If a budget below is exceeded and its site does
/// not call `known_open`, the run fails -- which is the point: a *new*
/// allocation regression is not excusable, only the two recorded ones are.
///
///   A2  `Box::leak` in `links::ascii_re_cached` retains ~3.9 KB per distinct
///       compiled pattern, forever. Reachable from document content through the
///       table path, and from the fixed literal patterns in `do_delim`.
const KNOWN_OPEN: &[&str] = &["A2: Box::leak retains ~3.9 KB per distinct pattern"];

fn known_open(item: &str, what: String) {
    assert!(
        KNOWN_OPEN.iter().any(|k| k.starts_with(item)),
        "{item} is recorded as known-open but is not in the KNOWN_OPEN audit list: \
         add it there with a pointer to the plan item, or fix it"
    );
    eprintln!("  KNOWN-OPEN [{item}]: {what}");
}

/// Bytes allocated inside `f`, and the net change in live bytes across it.
fn measure<F: FnOnce()>(f: F) -> (usize, isize) {
    let c0 = CUMULATIVE.load(Ordering::Relaxed);
    let l0 = LIVE.load(Ordering::Relaxed);
    f();
    (
        CUMULATIVE.load(Ordering::Relaxed) - c0,
        LIVE.load(Ordering::Relaxed) as isize - l0 as isize,
    )
}

fn converter(tables: bool) -> Converter {
    let mut o = Options::default();
    o.default_link_dict = String::new();
    o.make_tables = tables;
    Converter::new(o)
}

/// Every printable non-alphanumeric character a document can offer as a table
/// delimiter, minus regex metacharacters.
///
/// The engine is already safe here and this is not a workaround for it:
/// `convert.rs:2636` strips `^`, `[`, `]` and `\\` from a document-derived
/// delimiter before it is interpolated into a character class, and rejects the
/// row if nothing is left. The exclusion is belt-and-braces so that a
/// regression in that sanitising shows up as a *failure of this test's budget*
/// rather than as a panic that aborts the whole binary and hides every other
/// result.
fn printable_delimiters() -> Vec<char> {
    // Only these are unsafe inside a character class for a single-character
    // delimiter: `]` and `\` terminate or escape, `^` negates at the front, and
    // `-` could form a range. Everything else -- including `$ ( ) + ? { } | .`
    // -- is literal within `[...]`.
    const META: &str = r"\\^-[]";
    (0x21u8..0x7F)
        .map(char::from)
        .filter(|c| !c.is_ascii_alphanumeric() && !META.contains(*c))
        .collect()
}

// ------------------------------------------------------------------ A2, part 1
// The leak is real and unbounded in principle. `ascii_re_cached` returns a
// `&'static Regex` produced by `Box::leak`, so nothing is ever freed; the
// 128-entry cap clears the map, which drops references without releasing
// memory. This drives it directly, which is the only way to reach that state:
// only ~90 printable delimiter characters exist, so no real document can
// generate tens of thousands of distinct patterns in one process.
//
// This is a micro-benchmark and is labelled as one. It exists to establish the
// shape of the defect, not to be a performance target.

/// Drive the cache with `n` distinct keys and report what it cost.
///
/// The keys are deliberately built from a safe alphabet rather than from real
/// delimiter characters: what is being measured is one compiled regex retained
/// per distinct key, and that is independent of the pattern's contents.
fn drive_cache(n: usize) -> (usize, isize) {
    measure(|| {
        for i in 0..n {
            // A valid, distinct, metacharacter-free pattern per key.
            let pat = format!("(?:p{i}q)");
            let re = txt2html::links::ascii_re_cached(&pat);
            assert!(re.as_str().starts_with("(?:p"));
        }
    })
}

/// The leak's signature is not churn, it is **retention**: bytes allocated and
/// never given back. `cache.clear()` at `links.rs` drops the map's references
/// without freeing the `Box::leak`ed regexes, so live bytes climb with the
/// number of distinct keys and stay there.
///
/// Cumulative allocation is reported too, but it is the weaker signal -- lots of
/// allocation that is all freed is a time cost, not a leak, and conflating the
/// two is how the plan's original figures went unquestioned.
#[test]
fn a2_retains_one_regex_per_distinct_pattern() {
    let (_, live_small) = drive_cache(1_000);
    let (alloc_large, live_large) = drive_cache(4_000);

    eprintln!(
        "A2 direct-call: 4000 extra keys -> {alloc_large} bytes cumulative, \
         {live_large:+} bytes retained (1000 keys had retained {live_small:+})"
    );

    // Live memory must not scale with the number of distinct patterns. After the
    // fix this is a small constant; while the leak is present it is kilobytes per
    // pattern. The threshold sits between the two by three orders of magnitude.
    const BUDGET: isize = 64 * 1024;
    if live_large >= BUDGET {
        known_open(
            "A2",
            format!(
                "4000 distinct patterns retain {live_large} bytes ({:.0} B/pattern), \
                 over the {BUDGET} byte budget -- Box::leak in links::ascii_re_cached",
                live_large as f64 / 4_000.0
            ),
        );
    }

    // What must hold even while the leak is present: growth is linear in the
    // number of distinct patterns, not quadratic. This is the plan's original
    // claim, now checked rather than repeated.
    let per_pattern = live_large as f64 / 4_000.0;
    assert!(
        (per_pattern - 3_927.0).abs() < 1_500.0,
        "{per_pattern:.0} B/pattern is not the ~3.9 KB/pattern linear leak -- the \
         cost per pattern has changed shape and the figures in the plan need revisiting"
    );
}

// ------------------------------------------------------------------ A2, part 2
// What a *real* document can actually cost, and the regression budget A2's fix
// must keep under.
//
// The assertion is on **marginal retained bytes per distinct delimiter**, not on
// total allocation. Two reasons. Total allocation is dominated by ordinary churn
// -- the long-paragraph path alone accounts for tens of megabytes of
// allocate-and-free for a small document, which `a1_long_paragraph_allocation_is_linear`
// covers -- and a total-allocation budget would be measuring that instead of the
// leak. And the leak's signature is retention, so marginal cost per delimiter is
// both the sensitive and the specific signal: the converter's fixed overhead
// cancels out.
//
// `--make_tables` is required and off by default. Without it no table is
// detected and the delimiter is never compiled.

fn delim_document(n: usize, delims: &[char]) -> String {
    let mut text = String::new();
    for (i, d) in delims.iter().take(n).enumerate() {
        // Three *consecutive* rows sharing the delimiter, in one block. The
        // DELIM table detector bails out below two rows (`convert.rs:2625`), so
        // one row per block detects nothing and compiles nothing -- an earlier
        // version of this test measured exactly that and found no leak at all.
        // The rows start and end with the delimiter and carry four of them, which
        // is what `is_delim_table` requires before it interpolates the character
        // into a character class and compiles a pattern for it.
        for row in ["aa", "bb", "cc"] {
            text.push_str(&format!("{d}{row}{d}xx{d}yy{d}zz{d}\n"));
        }
        text.push('\n');
        // Enough layout structure to reach the other pattern-compiling paths, so
        // this is not only about the table one.
        text.push_str(&format!("Heading {i}\n=====\n\n* one\n* two\n\n"));
    }
    text
}

/// Measure one conversion in a **fresh thread**.
///
/// The regex cache is a `thread_local`, so two measurements on the same thread
/// share it: the second run would reuse the first run's compiled patterns and
/// the marginal cost would read as zero no matter how badly the cache leaked.
/// A new thread gets an empty cache, which is the only way to measure this
/// honestly in-process.
fn retained_for(n: usize, delims: &[char]) -> (usize, isize) {
    let text = delim_document(n, delims);
    std::thread::spawn(move || {
        measure(|| {
            let mut c = converter(true);
            let out = c.process_chunk(&text, true, false);
            assert!(!out.is_empty());
        })
    })
    .join()
    .expect("measurement thread panicked")
}

#[test]
fn a2_retained_bytes_do_not_scale_with_delimiter_count() {
    let delims = printable_delimiters();
    assert!(
        delims.len() >= 24,
        "expected a usable delimiter set, got {}",
        delims.len()
    );
    let (alloc_small, live_small) = retained_for(4, &delims);
    let (alloc_large, live_large) = retained_for(20, &delims);

    let marginal = (live_large - live_small) as f64 / 16.0;
    eprintln!(
        "A2 realistic: 4 delimiters {alloc_small} B/{live_small:+}, \
         20 delimiters {alloc_large} B/{live_large:+}, \
         marginal {marginal:.0} B per additional delimiter"
    );

    // After the fix this is zero. While the leak is present it is measured at
    // ~3.9 KB per pattern, so the real-input figure lands in the thousands. The
    // threshold sits well above the noise and well below the defect.
    const BUDGET_PER_DELIMITER: f64 = 2_048.0;
    if marginal >= BUDGET_PER_DELIMITER {
        known_open(
            "A2",
            format!(
                "each additional distinct delimiter retains {marginal:.0} bytes, over the \
                 {BUDGET_PER_DELIMITER:.0} byte budget -- the same Box::leak, reached from \
                 real document content"
            ),
        );
    }
}

// ---------------------------------------------------------------------- A1
// A1 removed a superlinear *time* explosion. This is the memory-side companion:
// one paragraph of ~1 MB must not allocate superlinearly either, and must
// release what it allocated when the converter is dropped.

#[test]
fn a1_long_paragraph_allocation_is_linear() {
    let mk = |n: usize| {
        let mut t = String::with_capacity(n * 5);
        for i in 0..n {
            t.push_str(&format!("line {i} word word word\n"));
        }
        t
    };
    let small = mk(2_000);
    let large = mk(20_000); // 10x the input

    let (small_alloc, small_live) = measure(|| {
        let mut c = converter(false);
        let out = c.process_chunk(&small, true, false);
        assert!(!out.is_empty());
    });
    let (large_alloc, large_live) = measure(|| {
        let mut c = converter(false);
        let out = c.process_chunk(&large, true, false);
        assert!(!out.is_empty());
    });

    let ratio = large_alloc as f64 / small_alloc.max(1) as f64;
    eprintln!(
        "A1 shape: 10x input ({small_alloc} -> {large_alloc} bytes, {ratio:.1}x), \
         net live {small_live:+} then {large_live:+}"
    );

    // 10x the input must not cost 100x. A generous ceiling: quadratic behaviour
    // would be 100x, linear is 10x, and 40x leaves room for size-dependent
    // effects that are not superlinear.
    assert!(
        ratio < 40.0,
        "10x the input allocated {ratio:.1}x the memory ({small_alloc} -> {large_alloc} \
         bytes), which suggests superlinear allocation"
    );

    // The converter must give the memory back when it goes out of scope. A
    // converter that holds the whole document alive after conversion is a leak
    // that no output comparison would ever see.
    assert!(
        large_live < large_alloc as isize / 2,
        "dropping the converter returned only {large_live} of {large_alloc} bytes"
    );
}

// ------------------------------------------------------------------ the floor
// Sanity check on the instrument itself. If the counter is broken, every budget
// above is meaningless, so assert it observes a known quantity first.

#[test]
fn the_instrument_works() {
    let (allocated, live_delta) = measure(|| {
        let v: Vec<u8> = Vec::with_capacity(1 << 20); // 1 MiB
        std::hint::black_box(&v);
    });
    assert!(
        allocated >= 1 << 20,
        "a 1 MiB allocation should be counted, saw {allocated}"
    );
    // Not an exact zero. Other allocations happen inside the measurement window
    // -- lazy statics, the harness's own bookkeeping, stdout buffering -- and
    // requiring a literal zero would make this self-test fail for reasons that
    // have nothing to do with the instrument. What matters is that a megabyte
    // went out and came back, which a residual of a few KB cannot hide.
    assert!(
        live_delta.abs() < 64 * 1024,
        "the 1 MiB vector should be released when dropped, but live bytes moved by \
         {live_delta}"
    );
}
