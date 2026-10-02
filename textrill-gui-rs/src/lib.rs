// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Native GUI for `textrill`, built with `egui`/`eframe`.
//!
//! The front end is deliberately thin: the option panel is generated from the
//! engine's own option table ([`textrill::cli::SPECS`]) so it cannot drift from
//! `--help`, and the text is converted by calling the engine directly rather
//! than through a binding.
//!
//! The widget drawing lives in [`TextrillApp::draw`], which takes an
//! `egui::Ui` and no `eframe` frame. That is the seam the acceptance tests use:
//! `egui_kittest` drives exactly the same code the window does.

#![forbid(unsafe_code)]

pub mod app;
pub mod document;
pub mod options_panel;
pub mod settings;
pub mod worker;

pub use app::{SaveAnswer, TextrillApp};
pub use document::Document;
pub use settings::Settings;
