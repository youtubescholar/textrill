# Licence notice for textrill

The full licence text is in [`LICENSE`](LICENSE): the GNU General Public
License, version 3. This file is the notice that accompanies it — why this
licence was chosen, whose copyright is involved, and what this project
changed — the companion file the GPL's own "How to Apply" appendix asks
distributors to add.

## Grant

This is a Rust reimplementation of txt2html, a Perl program originally written
by Seth Golub. It is a derivative work and is distributed under the terms of
the GNU General Public License, version 3 or (at your option) any later
version.

SPDX-License-Identifier: GPL-3.0-or-later

## Why GPLv3

The upstream distribution (txt2html 3.0) licenses itself "under the same terms
as Perl itself", which is the Artistic License 1.0 or the GNU General Public
License. Selecting the GPL branch of that dual grant is a choice this project
has made, not a requirement imposed by any dependency:

- Qt is not involved in this crate at all. textrill is pure Rust and
  links no Qt code, so nothing here is forced into a particular copyleft
  license by Qt.
- The GUI is pure Rust as well — `textrill-gui-rs`, built with `egui`/`eframe`
  — and links no Qt and no Python. It carries GPLv3 so that the port and its
  front end are covered by a single, consistent licence, which is easier for
  a user to reason about and easier for a distributor to ship. (The retired
  Python/PySide6 front end is not shipped; the design notes are in the
  repository's history.)

If you are redistributing this and would rather work under the LGPL, the
Artistic/GPL dual grant from upstream permits that, and no dependency imposes
an obstacle to it. That is a decision for you, not a permission this project
can grant on upstream's behalf beyond what upstream already allows.

## Upstream copyright

    Copyright 1994-2000 Seth Golub seth AT aigeek.com
    Copyright 2002-2013 Kathryn Andersen
    Copyright 2018-2019 Joao Eriberto Mota Filho

The original txt2html was written by Seth Golub, converted to a Perl
module by Kathryn Andersen, and packaged by Joao Eriberto Mota Filho.
The canonical source for the behaviour this port reproduces is the
txt2html distribution; see README.md and the `textrill/tests/corpus/`
harness, which compares this port's output byte for byte against the
reference.

## Modifications

    This Rust implementation, and the changes made to the behaviour described
    above, are Copyright (C) 2026 the textrill authors.

    Released under the GNU General Public License, version 3 or later, as
    permitted by the upstream grant quoted above.
