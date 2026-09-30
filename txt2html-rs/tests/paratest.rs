//! Mirrors upstream `t/10para.t`: the string-level API (process_para,
//! process_chunk, is_fragment) and the option handling for caps_tag and the
//! bold/italic delimiters.

use txt2html::convert::Converter;
use txt2html::options::Options;

fn converter() -> Converter {
    let mut o = Options::default();
    o.default_link_dict = String::new();
    Converter::new(o)
}

#[test]
fn process_para_basic() {
    let mut c = converter();
    let test_str = "Matty had a little truck\nhe drove it round and round\nand everywhere that Matty went\nthe truck was *always* found.\n";
    let ok_str = "<p>Matty had a little truck<br/>\nhe drove it round and round<br/>\nand everywhere that Matty went<br/>\nthe truck was <em>always</em> found.\n</p>";
    assert_eq!(c.process_para(test_str, true, false), ok_str);
}

#[test]
fn process_chunk_ordered_list() {
    let mut c = converter();
    let test_str = "Here is my list:\n\n1. Spam\n2. Jam\n3. Ham\n4. Pickles\n";
    let ok_str = "<p>Here is my list:\n</p><ol>\n  <li>Spam\n  </li><li>Jam\n  </li><li>Ham\n  </li><li>Pickles</li></ol>\n";
    assert_eq!(c.process_chunk(test_str, true, false), ok_str);
}

#[test]
fn process_chunk_empty_string() {
    let mut c = converter();
    assert_eq!(c.process_chunk("", true, false), "");
}

#[test]
fn process_chunk_is_fragment() {
    let mut c = converter();
    let test_str = "Matty had a little truck\nhe drove it round and round\nand everywhere that Matty went\nthe truck was *always* found.\n";
    let ok_str = "Matty had a little truck<br/>\nhe drove it round and round<br/>\nand everywhere that Matty went<br/>\nthe truck was <em>always</em> found.\n";
    assert_eq!(c.process_chunk(test_str, true, true), ok_str);
}

#[test]
fn process_para_fragment_with_url() {
    let mut c = converter();
    let ok_str = "I like to look at <a href=\"http://www.example.com\">http://www.example.com</a> a lot";
    assert_eq!(c.process_para("I like to look at http://www.example.com a lot", true, true), ok_str);
}

#[test]
fn process_chunk_caps_tag_off() {
    let mut o = Options::default();
    o.default_link_dict = String::new();
    o.caps_tag = String::new();
    let mut c = Converter::new(o);
    let test_str = "We have a line alone\nFULL OF CAPS AND FURY\n";
    let ok_str = "We have a line alone<br/>\nFULL OF CAPS AND FURY\n";
    assert_eq!(c.process_chunk(test_str, true, true), ok_str);
}

#[test]
fn process_chunk_custom_delimiters() {
    let mut o = Options::default();
    o.default_link_dict = String::new();
    o.bold_delimiter = "^".to_string();
    o.italic_delimiter = "--".to_string();
    let mut c = Converter::new(o);
    let test_str = "I am ^bold^,\nYou are --really krazy--.\n-----------------\n";
    let ok_str = "I am <strong>bold</strong>,<br/>\nYou are <em>really krazy</em>.\n<hr/>\n";
    assert_eq!(c.process_chunk(test_str, true, true), ok_str);
}

#[test]
fn process_chunk_no_delimiters() {
    let mut o = Options::default();
    o.default_link_dict = String::new();
    o.bold_delimiter = String::new();
    o.italic_delimiter = String::new();
    let mut c = Converter::new(o);
    let test_str = "I am ^bold^,\nYou are --really krazy--.\n-----------------\n";
    let ok_str = "I am ^bold^,<br/>\nYou are --really krazy--.\n<hr/>\n";
    assert_eq!(c.process_chunk(test_str, true, true), ok_str);
}

#[test]
fn instring_round_trip() {
    let mut o = Options::default();
    o.default_link_dict = String::new();
    o.instring = vec!["hello world\n".to_string()];
    let mut c = Converter::new(o);
    let out = c.txt2html();
    assert!(out.contains("<p>hello world</p>"), "unexpected output: {out:?}");
}
