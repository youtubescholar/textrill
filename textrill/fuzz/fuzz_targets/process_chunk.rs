#![no_main]

use libfuzzer_sys::fuzz_target;
use textrill::{Converter, Options};

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let s = String::from_utf8_lossy(data).into_owned();
    let mut opts = Options::default();
    opts.html5 = true;
    opts.close_optional_tags = true;
    opts.lower_case_tags = true;
    opts.demoronize = true;
    opts.escape_html_chars = true;
    let mut c = Converter::new(opts);
    let _ = c.process_chunk(&s, true, false);
    let _ = c.process_chunk(&s, true, true);
});
