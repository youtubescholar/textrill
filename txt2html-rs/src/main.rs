// txt2html — convert plain text to HTML.
//
// Copyright (C) 2026 the txt2html-rs authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

use std::process::ExitCode;

use txt2html::cli;
use txt2html::convert::Converter;
use txt2html::options::Options;

const PROG: &str = "txt2html";
const VERSION: &str = "3.0";

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

    if let Err(e) = cli::parse_args(&args, &mut opts) {
        eprintln!("{PROG}: {e}");
        eprintln!("{PROG}: try `{PROG} --help` for more information");
        return ExitCode::from(1);
    }

    // with no input, read standard input
    if opts.infile.is_empty() && opts.instring.is_empty() {
        opts.infile.push("-".to_string());
    }

    opts.deal_with_options();

    let mut conv = Converter::new(opts.clone());
    let out = conv.txt2html();

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
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
            // The reader went away; that is a normal way to stop, not a fault.
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: unable to open {},: {}", opts.outfile, e);
            ExitCode::from(1)
        }
    }
}
