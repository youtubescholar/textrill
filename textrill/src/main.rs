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
use textrill::options::Options;

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

    // P11: `@file`, `~/.txt2htmlrc` and `./.txt2htmlrc` are read before the
    // command line, so a command-line option always wins. The reference does the
    // same via `Getopt::ArgvFile::argvFile`.
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

    // A9: an input file that could not be opened is a failure, not something to
    // carry on from. The reference prints `Could not open …` and exits 0, so a
    // Makefile or CI step reads a 0-byte output file as a successful build. The
    // output itself is unchanged -- an unreadable file contributed nothing to it
    // either way -- but the exit code now says what happened.
    let (out, unreadable) = match conv.try_convert() {
        Ok(out) => (out, Vec::new()),
        Err(e) => (e.out, e.unreadable),
    };

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
        std::fs::write(&opts.outfile, out)
    };
    let wrote = match result {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
            // The reader went away; that is a normal way to stop, not a fault.
            true
        }
        Err(e) => {
            eprintln!("Error: unable to open {},: {}", opts.outfile, e);
            false
        }
    };

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
