#![no_main]

use libfuzzer_sys::fuzz_target;
use textrill::convert::decode_bytes_with;
use textrill::options::Encoding;

const ENCODINGS: [Encoding; 10] = [
    Encoding::Auto,
    Encoding::Utf8,
    Encoding::Cp1252,
    Encoding::Latin1,
    Encoding::Cp1251,
    Encoding::Cp1253,
    Encoding::Koi8R,
    Encoding::Utf16Le,
    Encoding::Utf16Be,
    Encoding::Utf32Le,
];

fuzz_target!(|data: &[u8]| {
    for encoding in ENCODINGS {
        let _ = decode_bytes_with(data, encoding);
    }
    let _ = decode_bytes_with(data, Encoding::Utf32Be);
});
