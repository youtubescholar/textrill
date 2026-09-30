// Copyright (C) 2026 the txt2html-rs authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Python bindings, exposed as `txt2html._native`.
//!
//! The Python side is a thin wrapper in `txt2html/__init__.py`; this module
//! only exposes the conversion itself plus the option metadata that the GUI
//! builds itself from.

use pyo3::exceptions::{PyKeyError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use crate::cli::{self, Kind};
use crate::convert::{read_any_file, Converter};
use crate::options::Options;

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Build an [`Options`] from a mapping of option names to values.
///
/// Values may be Python objects (`bool`, `int`, `str`, sequences) or, for
/// convenience, their command line spelling as a string. Names may be
/// abbreviated as on the command line; the `no` prefix negates booleans.
fn options_from_dict(dict: Option<&Bound<'_, PyDict>>) -> PyResult<Options> {
    let mut opts = Options::default();
    let Some(dict) = dict else {
        return Ok(opts);
    };
    for (key, value) in dict.iter() {
        let name: String = key.extract()?;
        let spec = cli::lookup(&name)
            .ok_or_else(|| PyKeyError::new_err(format!("Unknown option `{name}`")))?;
        let text = match (spec.kind, &value) {
            (Kind::Flag, _) => match value.extract::<bool>() {
                Ok(b) => (if b { "1" } else { "0" }).to_string(),
                Err(_) => value.str()?.extract::<String>()?,
            },
            (Kind::Int, _) => match value.extract::<i64>() {
                Ok(n) => n.to_string(),
                Err(_) => value.str()?.extract::<String>()?,
            },
            (Kind::TableType, _) => {
                if let Ok(flags) = value.downcast::<PyDict>() {
                    for (k, v) in flags.iter() {
                        let key: String = k.extract()?;
                        let on: bool = v.extract()?;
                        cli::set_table_type(
                            &mut opts,
                            &format!("{key}={}", if on { 1 } else { 0 }),
                        )
                        .map_err(PyValueError::new_err)?;
                    }
                    continue;
                }
                value.str()?.extract::<String>()?
            }
            (_, _) => match value.extract::<String>() {
                Ok(s) => s,
                Err(_) => {
                    // sequences (custom_heading_regexp, links_dictionaries, ...)
                    if spec.kind != Kind::StrArray {
                        return Err(PyValueError::new_err(format!(
                            "Option `{name}` does not take a list of values"
                        )));
                    }
                    let items: Vec<String> = value.extract().map_err(|_| {
                        PyValueError::new_err(format!(
                            "Option `{name}` expects a string or a list of strings"
                        ))
                    })?;
                    // clear first: a list replaces the default, it does not
                    // extend whatever an earlier run left behind
                    match spec.names[0] {
                        "custom_heading_regexp" => opts.custom_heading_regexp.clear(),
                        "infile" => opts.infile.clear(),
                        "instring" => opts.instring.clear(),
                        "links_dictionaries" => opts.links_dictionaries.clear(),
                        _ => {}
                    }
                    for item in items {
                        cli::set_value(&mut opts, &name, &item)
                            .map_err(PyValueError::new_err)?;
                    }
                    continue;
                }
            },
        };
        cli::set_value(&mut opts, &name, &text).map_err(PyValueError::new_err)?;
    }
    Ok(opts)
}

/// Convert with options that are already fully built, so no Python object is
/// touched while the GIL is released.
fn run(text: &str, mut opts: Options) -> String {
    // The text is converted the way a file would be: paragraph records, the
    // document header, append and prepend files.
    opts.infile.clear();
    opts.instring.clear();
    opts.outfile = String::new();
    opts.deal_with_options();
    Converter::new(opts).convert_text(text)
}

/// Convert a string to HTML. Returns the whole document unless `extract` is
/// set in `options`.
#[pyfunction]
#[pyo3(signature = (text, options = None))]
fn convert(py: Python<'_>, text: &str, options: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
    let opts = options_from_dict(options)?;
    Ok(py.allow_threads(|| run(text, opts)))
}

/// Read a file the way the command line tool does (UTF-8 when possible,
/// Latin-1 otherwise) and convert it.
#[pyfunction]
#[pyo3(signature = (path, options = None))]
fn convert_file(py: Python<'_>, path: &str, options: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
    let text = read_any_file(path)
        .ok_or_else(|| PyValueError::new_err(format!("Could not open {path}")))?;
    let opts = options_from_dict(options)?;
    Ok(py.allow_threads(|| run(&text, opts)))
}

/// Convert a single paragraph or fragment, without the surrounding document.
#[pyfunction]
#[pyo3(signature = (text, options = None))]
fn process_chunk(
    py: Python<'_>,
    text: &str,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<String> {
    let opts = options_from_dict(options)?;
    Ok(py.allow_threads(move || {
        let mut conv = Converter::new(opts);
        conv.process_chunk(text, true, true)
    }))
}

/// Metadata for every option, so a front end can build itself: the canonical
/// name, the accepted abbreviations, the value kind, the default value and a
/// one-line description.
#[pyfunction]
fn option_specs(py: Python<'_>) -> PyResult<Bound<'_, PyList>> {
    let defaults = Options::default();
    let list = PyList::empty(py);
    for spec in cli::SPECS {
        let default = cli::get_value(&defaults, spec.names[0])
            .map_err(|e| PyValueError::new_err(e))?;
        let kind = match spec.kind {
            Kind::Flag => "bool",
            Kind::Int => "int",
            Kind::TableType => "table_type",
            // list-valued options are reported separately: a front end has to
            // know that this one takes a list, not a single string
            Kind::StrArray => "str_array",
            Kind::Str => "str",
        };
        let aliases: Vec<&str> = spec.names[1..].to_vec();
        list.append((
            spec.names[0],
            aliases,
            kind,
            default,
            spec.help,
        ))?;
    }
    Ok(list)
}

/// The version of the underlying Rust library.
#[pyfunction]
fn version() -> &'static str {
    VERSION
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(convert, m)?)?;
    m.add_function(wrap_pyfunction!(convert_file, m)?)?;
    m.add_function(wrap_pyfunction!(process_chunk, m)?)?;
    m.add_function(wrap_pyfunction!(option_specs, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add("__version__", VERSION)?;
    // pyo3's PanicException inherits BaseException, not Exception, so Python code
    // that catches `Exception` misses every panic from the engine. It also
    // lives in the `pyo3_runtime` module, which is not importable, so it cannot
    // be reached by name without this. Re-exporting it means a front end can
    // write `except txt2html.PanicException` and mean it.
    m.add(
        "PanicException",
        m.py().get_type::<pyo3::panic::PanicException>(),
    )?;
    Ok(())
}
