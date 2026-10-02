"""txt2html — convert plain text to HTML.

A Rust port of `HTML::TextToHTML` 3.0 (and the ``txt2html`` script), wrapped
for Python.  The conversion is byte-identical to the original Perl module.

    >>> import txt2html
    >>> print(txt2html.convert("hello *world*", {"extract": True}).strip())
    <p>hello <em>world</em></p>

Options may be given as a mapping.  Names can be abbreviated exactly as on the
command line, booleans also accept the ``no`` prefix, and every value may be
given in its command line spelling::

    txt2html.convert(text, {"xhtml": False, "make_tables": True})
    txt2html.convert(text, {"no_xhtml": True, "make_tables": "1"})
    txt2html.convert(text, {"custom_heading_regexp": [r"^What: "]})

Use :func:`option_specs` to discover the available options, or :class:`Options`
for an object with attribute access and validation.
"""

from __future__ import annotations

from typing import Any, Iterable, Mapping

from . import _native

__all__ = [
    "Options",
    "PanicException",
    "convert",
    "convert_file",
    "option_specs",
    "process_chunk",
    "version",
    "__version__",
]

__version__ = _native.__version__

convert = _native.convert
convert_file = _native.convert_file
process_chunk = _native.process_chunk
option_specs = _native.option_specs

#: Raised when the engine panics.  It derives from :exc:`BaseException` and not
#: from :exc:`Exception`, so ``except Exception`` will not catch it -- that is
#: deliberate, because a panic is a bug rather than a rejected input, and code
#: that wants to survive one has to ask for it by name.
PanicException = _native.PanicException

#: Values the converter accepts for a false flag, matching the command line.
_FALSE = frozenset({"0", "no", "false", "off", ""})
_TRUE = frozenset({"1", "yes", "true", "on"})


def _as_bool(value: Any) -> bool:
    """Interpret a flag value the way the command line does."""
    if isinstance(value, str):
        lowered = value.strip().lower()
        if lowered in _FALSE:
            return False
        if lowered in _TRUE:
            return True
        raise ValueError(f"Invalid boolean value {value!r}")
    return bool(value)


def version() -> str:
    """Return the version of the underlying Rust library."""
    return _native.version()


class Options(Mapping[str, Any]):
    """A validated, ordered set of conversion options.

    Values are stored as Python objects and passed straight through to the
    extension module::

        opts = Options(xhtml=False, make_tables=True)
        opts["custom_heading_regexp"] = [r"^What: "]
        html = txt2html.convert(text, opts)

    Unknown names raise :exc:`KeyError`, values of the wrong type raise
    :exc:`ValueError`, and :attr:`changed` tells you whether anything differs
    from the defaults, which is handy for a "reset" button.
    """

    __slots__ = ("_values", "_specs")

    def __init__(self, **values: Any) -> None:
        self._specs = {spec[0]: spec for spec in option_specs()}
        self._values: dict[str, Any] = {}
        for name, value in values.items():
            self[name] = value

    # -- Mapping interface -------------------------------------------------

    def __getitem__(self, name: str) -> Any:
        if name not in self._specs:
            raise KeyError(name)
        return self._values.get(name, self._default(name))

    def __setitem__(self, name: str, value: Any) -> None:
        self._check(name, value)
        self._values[name] = value

    def __delitem__(self, name: str) -> None:
        if name not in self._specs:
            raise KeyError(name)
        self._values.pop(name, None)

    def __iter__(self):
        return iter(self._specs)

    def __len__(self) -> int:
        return len(self._specs)

    def __getattr__(self, name: str) -> Any:
        # attribute access for the option names
        if name in self._specs:
            return self[name]
        raise AttributeError(name)

    def __setattr__(self, name: str, value: Any) -> None:
        if name in self.__slots__:
            object.__setattr__(self, name, value)
            return
        if name in self._specs:
            self[name] = value
            return
        raise AttributeError(name)

    def __repr__(self) -> str:
        changed = ", ".join(f"{k}={v!r}" for k, v in self._values.items())
        return f"Options({changed})"

    # -- helpers -----------------------------------------------------------

    def _default(self, name: str) -> Any:
        spec = self._specs[name]
        kind, default = spec[2], spec[3]
        if kind == "bool":
            return _as_bool(default)
        if kind == "int":
            return int(default)
        if kind == "str":
            return default
        return [item for item in default.split("\n") if item]

    def _check(self, name: str, value: Any) -> None:
        if name not in self._specs:
            raise KeyError(f"Unknown option {name!r}")
        kind = self._specs[name][2]
        if kind == "bool":
            if not isinstance(value, (bool, int, str)):
                raise ValueError(f"Option {name!r} expects a boolean")
            _as_bool(value)
        elif kind == "int":
            if isinstance(value, bool) or not isinstance(value, (int, str)):
                raise ValueError(f"Option {name!r} expects a number")
            int(value)
        elif kind == "str":
            if not isinstance(value, str):
                raise ValueError(f"Option {name!r} expects a string")
        elif kind == "str_array":
            if isinstance(value, str) or not isinstance(value, Iterable):
                raise ValueError(
                    f"Option {name!r} expects a list of strings"
                    f" (a single string is allowed too)"
                )
            for item in value:
                if not isinstance(item, str):
                    raise ValueError(f"Option {name!r} expects a list of strings")
        elif kind == "table_type":
            raise ValueError(
                "table_type is set through make_tables and the per-type flags"
            )

    @property
    def changed(self) -> bool:
        """True when at least one option differs from its default."""
        return any(self[name] != self._default(name) for name in self._values)

    def as_dict(self) -> dict[str, Any]:
        """Every option, with its value resolved against the defaults.

        The result can be handed straight to :func:`convert`.
        """
        return {name: self[name] for name in self._specs}

    def changed_options(self) -> dict[str, Any]:
        """Only the options that differ from the defaults."""
        return {k: v for k, v in self._values.items() if v != self._default(k)}

    def reset(self) -> None:
        """Drop every change."""
        self._values.clear()

    def update(self, other: Mapping[str, Any] | Iterable[tuple[str, Any]]) -> None:
        """Set several options at once, like ``dict.update``."""
        items = other.items() if isinstance(other, Mapping) else other
        for name, value in items:
            self[name] = value
