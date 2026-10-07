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

    // --help / --version are handled before the full parse, like the
    // reference script.
    for a in &args {
        if a == "--help" || a == "-h" {
            print!("{}", cli::usage());
            return ExitCode::SUCCESS;
        }
        if a == "--version" {
            eprintln!("{PROG} version: {VERSION}");
            return ExitCode::SUCCESS;
        }
    }

    // P11: `@file`, `~/.textrillrc` and `./.textrillrc` (and the legacy
    // `.txt2htmlrc` names) are read before the command line, so a
    // command-line option always wins. The reference does the same via
    // `Getopt::ArgvFile::argvFile`.
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

    // Reject out-of-range values before converting anything, so the user gets a
    // message and a non-zero exit rather than a panic or an uncatchable abort.
    if let Err(e) = opts.validate() {
        eprintln!("{PROG}: {e}");
        return ExitCode::from(1);
    }

    let mut conv = Converter::new(opts.clone());

    // P5.0. `--report` counts the finished document, and a streaming run never
    // has one: it writes each paragraph and drops it, so there is nothing to
    // scan. Refusing rather than printing zeros, because a report of 0 headings
    // and 0 paragraphs from a document that plainly has them is a lie about the
    // input. This is the same class of refusal as `--number_headings`,
    // `--section`, `--toc` and `--chunk` below, which need the body in hand.
    if opts.report && opts.stream {
        eprintln!("{PROG}: --report is not valid with --stream");
        return ExitCode::from(1);
    }

    // P5.4. `--stream` feeds one paragraph at a time. It is only valid when
    // nothing needs the assembled body and the input is UTF-8, which is the
    // only decoding a reader can do without holding the whole file.
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
        return run_stream(&mut conv, &opts);
    }

    // P5.2. `--chunk` writes one file per top-level section, so it needs a real
    // output path to name the siblings next to and cannot be combined with the
    // single-document `--extract` mode or an in-memory `--instring`.
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
        let (files, unreadable) = conv.try_convert_chunked();
        let mut wrote = true;
        for (name, html) in &files {
            if let Err(e) = std::fs::write(name, html) {
                eprintln!("Error: unable to open {name}: {e}");
                wrote = false;
            }
        }
        // P5.0. One line for the whole run: `--chunk` writes several files and
        // the report is about what was produced, not about one of them. Placed
        // like the buffered path's -- after the writes, before the exit-status
        // checks -- so both say the same thing about a run that produced a
        // document and then failed.
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

    // A9: an input file that could not be opened is a failure, not something to
    // carry on from. The reference prints `Could not open …` and exits 0, so a
    // Makefile or CI step reads a 0-byte output file as a successful build. The
    // output itself is unchanged -- an unreadable file contributed nothing to it
    // either way -- but the exit code now says what happened.
    let (out, unreadable) = match conv.try_convert() {
        Ok(out) => (out, Vec::new()),
        Err(e) => (e.out, e.unreadable),
    };

    // A note set that cannot be rendered is a failure, and it is checked before
    // the output is opened. Writing the body anyway would leave `[1]` in the
    // prose pointing at nothing, which is exactly the failure the mode exists
    // to prevent -- and a half-converted document on disk is worse than none.
    if let Some(e) = &conv.notes_error {
        eprintln!("{PROG}: {e}");
        return ExitCode::from(1);
    }

    let result = if opts.outfile.is_empty() || opts.outfile == "-" {
        // Write through a locked handle so that a closed pipe (as in
        // `txt2html file.txt | head`) is reported as an ordinary error instead
        // of panicking, which is what the reference script does.
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
            // The reader went away; that is a normal way to stop, not a fault.
            true
        }
        Err(e) => {
            eprintln!("Error: unable to open {}: {}", opts.outfile, e);
            false
        }
    };

    // P5.0. After the write, so that the counts describe what reached the
    // output rather than what the converter held, and before the exit-status
    // checks, so that a run which produced a document and then failed on a
    // later input still reports it. Only on a successful write: a report of a
    // file that was never opened would be a report of nothing.
    if opts.report && wrote {
        eprintln!("{PROG}: report {}", Counts::of(&out));
    }

    // Reported after the write, so that a broken pipe (`txt2html f | head`) is
    // still exit 0 and a successful conversion of the readable inputs is not
    // lost behind the diagnostic.
    if !unreadable.is_empty() {
        // `try_convert` has already printed the reference's own
        // `Could not open …` line; this says what it means for the exit code.
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

/// P5.4. Run the streaming path for `--stream`: open the output once, then feed
/// each `--infile` through [`Converter::convert_stream`] in turn. Inputs are
/// decoded as UTF-8 (see that method); a file that cannot be opened contributes
/// nothing and makes the exit non-zero, mirroring the buffered path.
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
    for f in &opts.infile {
        let result = if f == "-" {
            conv.convert_stream(BufReader::new(stdin.lock()), &mut writer)
        } else {
            match std::fs::File::open(f) {
                Ok(file) => conv.convert_stream(BufReader::new(file), &mut writer),
                Err(_) => {
                    // Same line the reference prints for an unopenable input.
                    eprintln!("Could not open {f}\n");
                    unreadable += 1;
                    continue;
                }
            }
        };
        match result {
            Ok(()) => {}
            // `textrill file | head` is a normal way to stop, not a fault.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => return ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{PROG}: {e}");
                return ExitCode::from(1);
            }
        }
    }

    if let Err(e) = writer.flush() {
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            eprintln!("{PROG}: {e}");
            return ExitCode::from(1);
        }
    }
    if unreadable > 0 {
        eprintln!("{PROG}: could not read {unreadable} input file(s), exiting non-zero");
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
