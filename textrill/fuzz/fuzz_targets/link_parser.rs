#![no_main]

use libfuzzer_sys::fuzz_target;
use textrill::links::load_links_from_text;
use textrill::options::Options;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data).into_owned();
    let opts = Options::default();
    let mut lp = load_links_from_text(&opts, &s);
    lp.parse_dict("fuzz.dict", &s);
});
