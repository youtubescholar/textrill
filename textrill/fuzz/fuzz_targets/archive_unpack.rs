#![no_main]

use libfuzzer_sys::fuzz_target;
use std::env;
use textrill::pack::archive;

fuzz_target!(|data: &[u8]| {
    let dir = env::temp_dir().join(format!("textrill-fuzz-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let _ = archive::unpack(data, &dir);
    let _ = std::fs::remove_dir_all(&dir);
});
