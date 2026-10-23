use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;
use std::time::Duration;
use textrill::{Converter, Options};

const BLOCK: &str = "\
Chapter One
===========

This is a paragraph with *emphasis*, a link http://example.org and ordinary
prose that wraps across lines. It should be inferred as a paragraph.

- first item
- second item with a nested line
  - nested item
- third item

    preformatted text
    stays verbatim

Name: value
Other: value

";

fn document(bytes: usize) -> String {
    let mut text = String::with_capacity(bytes + BLOCK.len());
    while text.len() < bytes {
        text.push_str(BLOCK);
    }
    text
}

fn convert(c: &mut Criterion) {
    let options = Options::default();
    for size in [64 * 1024, 1024 * 1024] {
        let input = document(size);
        let mut group = c.benchmark_group(format!("convert/{}KiB", input.len() / 1024));
        group.throughput(Throughput::Bytes(input.len() as u64));
        group.sample_size(10);
        let measurement_time = if size >= 1024 * 1024 { 25 } else { 10 };
        group.measurement_time(Duration::from_secs(measurement_time));
        group.bench_function("convert_text", |b| {
            b.iter(|| {
                let mut converter = Converter::new(options.clone());
                black_box(converter.convert_text(black_box(&input)));
            });
        });
        group.finish();
    }
}

criterion_group!(benches, convert);
criterion_main!(benches);
