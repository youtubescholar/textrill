# txt2html GUI — a Qt front end for the txt2html converter.
#
# Copyright (C) 2026 the txt2html-rs authors.
#
# This program is free software: you can redistribute it and/or modify it
# under the terms of the GNU General Public License as published by the Free
# Software Foundation, either version 3 of the License, or (at your option)
# any later version.  See the LICENSE file for the full text.

"""Running conversions off the GUI thread.

The extension module releases the GIL while it converts, so a long document
does not freeze the interface. Results carry a generation number: a result
that arrives after a newer conversion was requested is dropped, which keeps
the preview from flickering with stale output.
"""

from __future__ import annotations

import time
import traceback
from typing import Any, Dict, Optional

from PySide6.QtCore import QObject, QRunnable, QThreadPool, Signal, Slot


class Conversion(QObject):
    """The result of one conversion."""

    finished = Signal(int, str, float, str)
    """generation, html, seconds, error message"""


class _Job(QRunnable):
    def __init__(self, generation: int, text: str, options: Dict[str, Any],
                 sink: Conversion) -> None:
        super().__init__()
        self.generation = generation
        self.text = text
        self.options = options
        self.sink = sink

    @Slot()
    def run(self) -> None:  # executed on a worker thread
        import txt2html

        started = time.perf_counter()
        html = ""
        error = ""
        try:
            html = txt2html.convert(self.text, self.options)
        except txt2html.PanicException:
            # A panic inside the engine. It is a BaseException, so a plain
            # `except Exception` does not see it and the result signal below
            # would never be emitted, leaving the window showing "converting…"
            # forever with no indication anything went wrong.
            error = (
                "The converter stopped on invalid input.\n\n"
                f"{traceback.format_exc(limit=0).strip()}\n\n"
                "This is a bug in txt2html. The input is probably the cause."
            )
        except Exception:  # keep the GUI alive whatever the input is
            error = traceback.format_exc(limit=3)
        finally:
            # Emitted from `finally` so that no failure above, including an
            # unforeseen BaseException, can leave the window waiting.
            try:
                self.sink.finished.emit(
                    self.generation, html, time.perf_counter() - started, error
                )
            except RuntimeError:
                pass  # the window went away while this job was running


class Converter:
    """Queues conversions and hands the results back through a signal."""

    def __init__(self, parent: Optional[QObject] = None, max_threads: int = 2) -> None:
        self.pool = QThreadPool()
        self.pool.setMaxThreadCount(max_threads)
        # deliberately not parented: the sink has to outlive the window so a
        # worker that finishes during shutdown can still deliver its result
        self.sink = Conversion()
        self._generation = 0

    @property
    def generation(self) -> int:
        return self._generation

    def convert(self, text: str, options: Dict[str, Any]) -> int:
        """Queue a conversion; returns its generation number.

        Anything still waiting in the queue is dropped first. Only the newest
        text is worth converting: the window discards a stale *result* already,
        but without this it went on doing the stale *work*, and a keystroke burst
        queues faster than the pool drains. Each queued job holds its own copy of
        the document, so a backlog is also a memory backlog.

        `QThreadPool.clear` removes runnables that have not started. A job already
        on a thread cannot be recalled, so at most `max_threads` conversions are
        ever in flight and the rest of the queue is dropped at each keystroke
        rather than growing for as long as the user types.
        """
        self._generation += 1
        self.pool.clear()
        job = _Job(self._generation, text, options, self.sink)
        self.pool.start(job)
        return self._generation

    def cancel_pending(self) -> None:
        """Forget queued work that has not started yet."""
        self.pool.clear()

    def wait(self, msecs: int = 30000) -> bool:
        """Block until the queue drains; used when shutting down."""
        return self.pool.waitForDone(msecs)
