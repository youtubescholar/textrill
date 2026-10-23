#![no_main]

use libfuzzer_sys::fuzz_target;
use textrill::pack::manifest;

fuzz_target!(|data: &[u8]| {
    let s = String::from_utf8_lossy(data).into_owned();
    let _ = manifest::parse(&s);
});
