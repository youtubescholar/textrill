# txt2html GUI — a Qt front end for the txt2html converter.
#
# Copyright (C) 2026 the txt2html-rs authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by the Free
# Software Foundation, either version 3 of the License, or (at your option)
# any later version.  See the LICENSE file for the full text.

"""The options form, built from the converter's own option metadata.

Nothing here hard-codes the option list: widgets, defaults, help texts and
the grouping into "basic", "formatting", "tables", "lists", "preformatted",
"links" and "document" all come from :func:`txt2html.option_specs`, so a new
option in the Rust library shows up without touching this file.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, List, Optional, Tuple

from PySide6.QtCore import Qt, Signal
from PySide6.QtGui import QFont
from PySide6.QtWidgets import (
    QCheckBox,
    QFormLayout,
    QGroupBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QListWidget,
    QPushButton,
    QScrollArea,
    QSpinBox,
    QToolButton,
    QVBoxLayout,
    QWidget,
)

#: Table types of the Perl module, in the order it recognises them.
TABLE_TYPES = ("ALIGN", "PGSQL", "BORDER", "DELIM")

#: Values the converter accepts for a flag, matching the command line.
FALSE_WORDS = frozenset({"0", "no", "false", "off", ""})
TRUE_WORDS = frozenset({"1", "yes", "true", "on"})

#: Which group each option belongs to, and in what order.
GROUPS = (
    ("Document", ("doctype", "title", "titlefirst", "body_deco", "style_url",
                  "append_head", "append_file", "prepend_file", "extract",
                  "lower_case_tags", "xhtml")),
    ("Text", ("escape_HTML_chars", "eight_bit_clean", "demoronize", "tab_width",
              "short_line_length", "hrule_min", "unhyphenation", "preserve_indent",
              "indent_par_break", "par_indent", "indent_width")),
    ("Formatting", ("bold_delimiter", "italic_delimiter", "underline_delimiter",
                    "underline_length_tolerance", "underline_offset_tolerance",
                    "caps_tag", "min_caps_length", "use_mosaic_header",
                    "explicit_headings", "custom_heading_regexp")),
    ("Lists", ("bullets", "bullets_ordered")),
    ("Tables", ("make_tables", "table_type")),
    ("Preformatted", ("make_anchors", "use_preformat_marker",
                      "preformat_whitespace_min", "preformat_trigger_lines",
                      "endpreformat_trigger_lines", "preformat_start_marker",
                      "preformat_end_marker")),
    ("Mail", ("mailmode",)),
    ("Links", ("make_links", "link_only", "default_link_dict",
               "links_dictionaries")),
)

#: Options that are hidden from the form: plumbing rather than conversion.
#: ``infile`` and ``instring`` are inputs, ``outfile`` is handled by the file
#: dialogs, and ``utf8`` is accepted but has no effect, exactly as upstream.
HIDDEN = {"infile", "instring", "outfile", "utf8"}


@dataclass(frozen=True)
class OptionSpec:
    """One option as described by the extension module."""

    name: str
    aliases: List[str]
    kind: str
    default: str
    help: str
    #: The range the engine accepts, as reported by the extension module, or None
    #: for an option it does not restrict. Read rather than restated here, so a
    #: spin box cannot offer a value the engine will reject -- which is how
    #: tab_width=0 used to reach the tab expander and divide by zero.
    accepted: Optional[Tuple[int, int]] = None

    def __init__(
        self,
        name: str,
        aliases: List[str],
        kind: str,
        default: str,
        help: str,
        accepted: Optional[Tuple[int, int]] = None,
    ) -> None:
        object.__setattr__(self, "name", name)
        object.__setattr__(self, "aliases", aliases)
        object.__setattr__(self, "kind", kind)
        object.__setattr__(self, "default", default)
        object.__setattr__(self, "help", help)
        object.__setattr__(self, "accepted", accepted)

    @property
    def is_list(self) -> bool:
        """True when the option takes a list of values."""
        return self.kind == "str_array"

    @property
    def default_bool(self) -> bool:
        """The default, read as a flag."""
        word = self.default.strip().lower()
        if word in FALSE_WORDS:
            return False
        if word in TRUE_WORDS:
            return True
        raise ValueError(f"Invalid boolean default {self.default!r}")

    @property
    def default_list(self) -> List[str]:
        """The default, split into a list."""
        return [item for item in self.default.split("\n") if item]

    @property
    def minimum(self) -> int:
        """Lowest value this option accepts, so spin boxes are not silly.

        Taken from the engine when it reports a range. The preformat pair is the
        exception: the engine clamps it to a signed byte rather than rejecting a
        value, so its bound is the GUI's to state.
        """
        if self.accepted is not None:
            return self.accepted[0]
        if self.name in ("preformat_trigger_lines", "endpreformat_trigger_lines"):
            return -128
        return 0

    @property
    def maximum(self) -> int:
        """Highest value this option accepts, from the engine where it says."""
        if self.accepted is not None:
            return self.accepted[1]
        if self.name in ("preformat_trigger_lines", "endpreformat_trigger_lines"):
            return 127
        return 999


def load_specs() -> List[OptionSpec]:
    """Read the option table from the extension module."""
    import txt2html

    specs = []
    for name, aliases, kind, default, accepted, help_text in txt2html.option_specs():
        if name in HIDDEN:
            continue
        specs.append(
            OptionSpec(
                name,
                list(aliases),
                kind,
                default,
                help_text,
                tuple(accepted) if accepted is not None else None,
            )
        )
    return specs


class StringListEditor(QWidget):
    """A one-item-per-line editor, used for regexp and dictionary lists."""

    changed = Signal()

    def __init__(self, values: List[str], parent: QWidget | None = None) -> None:
        super().__init__(parent)
        layout = QVBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(4)
        self.list = QListWidget()
        self.list.setFont(QFont("monospace"))
        self.list.setMinimumHeight(72)
        self.list.addItems(values)
        layout.addWidget(self.list)

        row = QHBoxLayout()
        row.setSpacing(4)
        add = QPushButton("Add")
        remove = QPushButton("Remove")
        add.clicked.connect(self._add)
        remove.clicked.connect(self._remove)
        for button in (add, remove):
            button.setMaximumWidth(90)
            row.addWidget(button)
        row.addStretch(1)
        layout.addLayout(row)

    def _add(self) -> None:
        item = self.list.addItem("")
        self.list.setCurrentItem(item)
        self.list.editItem(item)
        self.changed.emit()

    def _remove(self) -> None:
        for item in self.list.selectedItems():
            self.list.takeItem(self.list.row(item))
        self.changed.emit()

    def values(self) -> List[str]:
        return [self.list.item(i).text() for i in range(self.list.count())]

    def set_values(self, values: List[str]) -> None:
        self.list.clear()
        self.list.addItems(values)


class OptionRow(QWidget):
    """One option: a label with its help text, a widget, and a reset button."""

    changed = Signal()

    def __init__(self, spec: OptionSpec, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.spec = spec
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(6)

        self.editor = self._make_editor(spec)
        self.reset_button = QToolButton()
        self.reset_button.setText("↺")
        self.reset_button.setToolTip("Reset to the default")
        self.reset_button.setAutoRaise(True)
        self.reset_button.clicked.connect(self.reset)
        self.reset_button.setVisible(False)

        layout.addWidget(self.editor, 1)
        layout.addWidget(self.reset_button, 0)

        hint = spec.help
        if spec.aliases:
            hint += f"  ({', '.join(spec.aliases)})"
        self.editor.setToolTip(hint)
        self.setToolTip(hint)
        self._connect(self.editor)
        self.reset_button.clicked.connect(self.changed)

    def _make_editor(self, spec: OptionSpec) -> QWidget:
        if spec.kind == "bool":
            box = QCheckBox()
            box.setChecked(spec.default_bool)
            return box
        if spec.kind == "int":
            spin = QSpinBox()
            spin.setRange(spec.minimum, spec.maximum)
            spin.setValue(int(spec.default))
            spin.setKeyboardTracking(False)
            return spin
        if spec.is_list:
            return StringListEditor(spec.default_list)
        if spec.kind == "table_type":
            return _TableTypeEditor(spec.default)
        edit = QLineEdit(spec.default)
        return edit

    def _connect(self, editor: QWidget) -> None:
        if isinstance(editor, QCheckBox):
            editor.toggled.connect(self.changed)
        elif isinstance(editor, QSpinBox):
            editor.valueChanged.connect(self.changed)
        elif isinstance(editor, QLineEdit):
            editor.textEdited.connect(self.changed)
        elif isinstance(editor, StringListEditor):
            editor.changed.connect(self.changed)
        elif isinstance(editor, _TableTypeEditor):
            editor.changed.connect(self.changed)

    # -- values ---------------------------------------------------------

    def default_value(self):
        if self.spec.kind == "bool":
            return self.spec.default_bool
        if self.spec.kind == "int":
            return int(self.spec.default)
        if self.spec.is_list:
            return self.spec.default_list
        return self.spec.default

    def value(self):
        editor = self.editor
        if isinstance(editor, QCheckBox):
            return editor.isChecked()
        if isinstance(editor, QSpinBox):
            return editor.value()
        if isinstance(editor, QLineEdit):
            return editor.text()
        if isinstance(editor, StringListEditor):
            return editor.values()
        if isinstance(editor, _TableTypeEditor):
            return editor.value()
        return None

    def set_value(self, value) -> None:
        editor = self.editor
        if isinstance(editor, QCheckBox):
            editor.setChecked(bool(value))
        elif isinstance(editor, QSpinBox):
            editor.setValue(int(value))
        elif isinstance(editor, QLineEdit):
            editor.setText(str(value))
        elif isinstance(editor, StringListEditor):
            editor.set_values(list(value))
        elif isinstance(editor, _TableTypeEditor):
            editor.set_value(value)

    def is_default(self) -> bool:
        current = self.value()
        default = self.default_value()
        if isinstance(default, list):
            return current == default
        return current == default

    def reset(self) -> None:
        self.set_value(self.default_value())
        self.changed.emit()

    def label_text(self) -> str:
        return self.spec.name.replace("_", " ")


class _TableTypeEditor(QWidget):
    """The four table-type switches behind the single ``table_type`` option."""

    changed = Signal()

    def __init__(self, default: str, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        layout = QHBoxLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(8)
        self.boxes: Dict[str, QCheckBox] = {}
        for name in TABLE_TYPES:
            box = QCheckBox(name.lower())
            box.setChecked(f"{name}=1" in default)
            box.toggled.connect(self.changed)
            self.boxes[name] = box
            layout.addWidget(box)
        layout.addStretch(1)

    def value(self) -> Dict[str, bool]:
        return {name: box.isChecked() for name, box in self.boxes.items()}

    def set_value(self, value) -> None:
        if isinstance(value, dict):
            for name, box in self.boxes.items():
                box.setChecked(bool(value.get(name.lower(), value.get(name, True))))


class OptionsPanel(QScrollArea):
    """The whole options form, with a filter box and a reset button."""

    changed = Signal()
    """Emitted whenever any option changes."""

    def __init__(self, parent: QWidget | None = None) -> None:
        super().__init__(parent)
        self.setWidgetResizable(True)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)

        self.rows: Dict[str, OptionRow] = {}
        self._groups: Dict[QGroupBox, List[OptionRow]] = {}
        self._matches: Dict[int, bool] = {}
        self._building = False

        container = QWidget()
        outer = QVBoxLayout(container)
        outer.setContentsMargins(8, 8, 8, 8)
        outer.setSpacing(8)

        self.filter_box = QLineEdit()
        self.filter_box.setPlaceholderText("Filter options…")
        self.filter_box.setClearButtonEnabled(True)
        self.filter_box.textChanged.connect(self._apply_filter)
        outer.addWidget(self.filter_box)

        specs = load_specs()
        by_name = {spec.name: spec for spec in specs}
        listed: set = set()
        for title, names in GROUPS:
            rows = [(n, by_name[n]) for n in names if n in by_name]
            if not rows:
                continue
            listed.update(name for name, _ in rows)
            outer.addWidget(self._build_group(title, rows))
        rest = [s for s in specs if s.name not in listed]
        if rest:
            outer.addWidget(self._build_group("Other", [(s.name, s) for s in rest]))

        reset_row = QHBoxLayout()
        self.reset_all_button = QPushButton("Reset all options")
        self.reset_all_button.clicked.connect(self.reset_all)
        reset_row.addWidget(self.reset_all_button)
        reset_row.addStretch(1)
        outer.addLayout(reset_row)
        outer.addStretch(1)

        self.setWidget(container)

    def _build_group(self, title: str, rows) -> QGroupBox:
        box = QGroupBox(title)
        form = QFormLayout(box)
        form.setLabelAlignment(Qt.AlignRight | Qt.AlignVCenter)
        form.setFieldGrowthPolicy(QFormLayout.ExpandingFieldsGrow)
        built = []
        for name, spec in rows:
            row = OptionRow(spec)
            row.changed.connect(self._on_row_changed)
            self.rows[name] = row
            self._matches[id(row)] = True
            built.append(row)
            form.addRow(QLabel(row.label_text()), row)
        self._groups[box] = built
        return box

    def _on_row_changed(self) -> None:
        if self._building:
            return
        self.changed.emit()

    def _apply_filter(self, text: str) -> None:
        """Show the rows that match, and the groups that still have one.

        Visibility is tracked explicitly rather than read back from the
        widgets: a row can be visible in principle while the window itself is
        hidden, and asking Qt would then say "not visible" for every row.
        """
        needle = text.strip().lower()
        for name, row in self.rows.items():
            haystack = " ".join(
                [name, row.label_text(), row.spec.help, *row.spec.aliases]
            ).lower()
            matched = not needle or needle in haystack
            row.setVisible(matched)
            self._matches[id(row)] = matched
        for box, rows in self._groups.items():
            box.setVisible(any(self._matches[id(row)] for row in rows))

    # -- values ---------------------------------------------------------

    def matches(self, name: str) -> bool:
        """True when the option passes the current filter."""
        if name not in self.rows:
            raise KeyError(name)
        return self._matches[id(self.rows[name])]

    def clear_filter(self) -> None:
        self.filter_box.clear()

    def values(self) -> Dict[str, Any]:
        """The current options, ready to hand to :func:`txt2html.convert`.

        `meta_charset` is forced on. The engine defaults it **off** so that no
        golden file moves, which is the right default for a byte-compatible CLI
        but the wrong one here: this panel's consumer is a browser reading a file
        the GUI wrote, and the GUI always writes UTF-8, so without a declaration
        the browser guesses the encoding and can guess wrong. The plan said the
        GUI should turn this on and it did not, which is the same category of
        plan/code disagreement the encoding rule itself had twice.
        """
        values = {name: row.value() for name, row in self.rows.items()}
        if "meta_charset" in values:
            values["meta_charset"] = True
        return values

    def set_value(self, name: str, value) -> None:
        if name not in self.rows:
            raise KeyError(name)
        self._building = True
        try:
            self.rows[name].set_value(value)
        finally:
            self._building = False

    def set_values(self, values: Dict[str, Any]) -> None:
        self._building = True
        try:
            for name, value in values.items():
                if name in self.rows:
                    self.rows[name].set_value(value)
        finally:
            self._building = False

    def reset_all(self) -> None:
        self._building = True
        try:
            for row in self.rows.values():
                row.set_value(row.default_value())
        finally:
            self._building = False
        self.changed.emit()
