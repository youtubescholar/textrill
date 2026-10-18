#![forbid(unsafe_code)]
// textrill — convert plain text to HTML.
//
// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

use std::process::ExitCode;

use textrill::cli;
use textrill::convert::Converter;
use textrill::options::{Encoding, Options};
use textrill::report::Counts;

const PROG: &str = "textrill";
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = Options::default();

    // --help / --version handled before rc files: a broken rc file cannot get
    // in the way of asking for help. The scan is token-aware (sees the `--`
    // terminator and option values, unlike a plain arg sweep), so it only fires
    // on a genuine option token -- `-- --version` is a file, not a request.
    match cli::early_action(&args) {
        cli::EarlyAction::None => {}
        cli::EarlyAction::Help => {
            print!("{}", cli::usage());
            return ExitCode::SUCCESS;
        }
        cli::EarlyAction::Version => {
            eprintln!("{PROG} version: {VERSION}");
            return ExitCode::SUCCESS;
        }
    }

    // rc files and @file processed before command line; command line wins.
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    if let Err(e) = cli::parse_args_with_rc(&args, &mut opts, home.as_deref(), &cwd) {
        eprintln!("{PROG}: {e}");
        eprintln!("{PROG}: try `{PROG} --help` for more information");
        return ExitCode::from(1);
    }

    // with no input, read standard input
    if opts.infile.is_empty() && opts.instring.is_empty() {
        opts.infile.push("-".to_string());
    }

    opts.deal_with_options();

    // Validate options before conversion.
    if let Err(e) = opts.validate() {
        eprintln!("{PROG}: {e}");
        return ExitCode::from(1);
    }

    // --report not valid with --stream (needs full document).
    if opts.report && opts.stream {
        eprintln!("{PROG}: --report is not valid with --stream");
        return ExitCode::from(1);
    }

    // --stream requires no body-wide modes and UTF-8/auto input.
    if opts.stream {
        if !opts.instring.is_empty() {
            eprintln!("{PROG}: --stream is not valid with --instring");
            return ExitCode::from(1);
        }
        if opts.chunk {
            eprintln!("{PROG}: --stream is not valid with --chunk");
            return ExitCode::from(1);
        }
        if opts.section || opts.toc {
            eprintln!("{PROG}: --stream is not valid with --section or --toc");
            return ExitCode::from(1);
        }
        if opts.number_headings {
            eprintln!("{PROG}: --stream is not valid with --number_headings");
            return ExitCode::from(1);
        }
        if !matches!(opts.encoding, Encoding::Auto | Encoding::Utf8) {
            eprintln!(
                "{PROG}: --stream needs UTF-8 input; --encoding {} needs the whole file",
                opts.encoding.name()
            );
            return ExitCode::from(1);
        }
        // Constructed only after the refusals, so a refused invocation never
        // loads dictionaries or a template for nothing.
        let mut conv = Converter::new(opts.clone());
        return run_stream(&mut conv, &opts);
    }

    // --chunk requires output file and no extract; not compatible with instring.
    if opts.chunk {
        if opts.extract {
            eprintln!("{PROG}: --chunk is not valid with --extract");
            return ExitCode::from(1);
        }
        if !opts.instring.is_empty() {
            eprintln!("{PROG}: --chunk is not valid with --instring");
            return ExitCode::from(1);
        }
        if opts.outfile.is_empty() || opts.outfile == "-" {
            eprintln!("{PROG}: --chunk requires --outfile");
            return ExitCode::from(1);
        }
        let mut conv = Converter::new(opts.clone());
        let (files, unreadable) = conv.try_convert_chunked();
        let mut wrote = true;
        for (name, html) in &files {
            if let Err(e) = std::fs::write(name, html) {
                eprintln!("Error: unable to open {name}: {e}");
                wrote = false;
            }
        }
        // --report aggregates over all chunk files when successful.
        if opts.report && wrote {
            let mut total = Counts::default();
            for (_, html) in &files {
                total.add(&Counts::of(html));
            }
            eprintln!("{PROG}: report {total}");
        }
        if !unreadable.is_empty() {
            eprintln!(
                "{PROG}: could not read {} input file(s), exiting non-zero",
                unreadable.len()
            );
            return ExitCode::from(1);
        }
        return if wrote {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        };
    }

    // Unreadable input causes non-zero exit. Output for readable files unchanged.
    let mut conv = Converter::new(opts.clone());
    let (out, unreadable) = match conv.try_convert() {
        Ok(out) => (out, Vec::new()),
        Err(e) => (e.out, e.unreadable),
    };

    // Notes rendering failure prevents writing output (covered by tests).
    if let Some(e) = &conv.notes_error {
        eprintln!("{PROG}: {e}");
        return ExitCode::from(1);
    }

    let result = if opts.outfile.is_empty() || opts.outfile == "-" {
        // Write to stdout; treat broken pipe as non-error.
        use std::io::Write;
        let stdout = std::io::stdout();
        let mut out_handle = stdout.lock();
        match out_handle.write_all(out.as_bytes()) {
            Ok(()) => out_handle.flush(),
            Err(e) => Err(e),
        }
    } else {
        std::fs::write(&opts.outfile, &out)
    };
    let wrote = match result {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
            // Broken pipe: normal termination, treat as success.
            true
        }
        Err(e) => {
            eprintln!("Error: unable to open {}: {}", opts.outfile, e);
            false
        }
    };

    // Report counts after successful write.
    if opts.report && wrote {
        eprintln!("{PROG}: report {}", Counts::of(&out));
    }

    // Check unreadable inputs after reporting.
    if !unreadable.is_empty() {
        // Indicates unreadable inputs cause non-zero exit.
        eprintln!(
            "{PROG}: could not read {} input file(s), exiting non-zero",
            unreadable.len()
        );
        return ExitCode::from(1);
    }
    if !wrote {
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

/// Streaming path for `--stream`: write paragraph by paragraph.
fn run_stream(conv: &mut Converter, opts: &Options) -> ExitCode {
    use std::io::{BufReader, BufWriter, Write};

    let mut writer: Box<dyn Write> = if opts.outfile.is_empty() || opts.outfile == "-" {
        Box::new(BufWriter::new(std::io::stdout()))
    } else {
        match std::fs::File::create(&opts.outfile) {
            Ok(file) => Box::new(BufWriter::new(file)),
            Err(e) => {
                eprintln!("Error: unable to open {}: {}", opts.outfile, e);
                return ExitCode::from(1);
            }
        }
    };

    let stdin = std::io::stdin();
    let mut unreadable = 0usize;
    let mut broken_pipe = false;
    for f in &opts.infile {
        let result = if f == "-" {
            conv.convert_stream(BufReader::new(stdin.lock()), &mut writer)
        } else {
            match std::fs::File::open(f) {
                Ok(file) => conv.convert_stream(BufReader::new(file), &mut writer),
                Err(_) => {
                    eprintln!("Could not open {f}\n");
                    unreadable += 1;
                    continue;
                }
            }
        };
        match result {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                // The reader is gone; nothing more can be written. Record it
                // rather than returning now, so an unreadable input earlier in
                // the list still exits non-zero -- the buffered path does the
                // same, and a worker that swallowed an unreadable file would
                // hide the failure from the pipeline that most needs to see it.
                broken_pipe = true;
                break;
            }
            Err(e) => {
                eprintln!("{PROG}: {e}");
                return ExitCode::from(1);
            }
        }
    }

    if unreadable > 0 {
        eprintln!("{PROG}: could not read {unreadable} input file(s), exiting non-zero");
        return ExitCode::from(1);
    }
    if broken_pipe {
        return ExitCode::SUCCESS;
    }
    if let Err(e) = writer.flush() {
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            eprintln!("{PROG}: {e}");
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}
