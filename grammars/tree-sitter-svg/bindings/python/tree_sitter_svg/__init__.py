"""SVG grammar for Tree-sitter"""

from importlib.resources import files as _files
from typing import TYPE_CHECKING

from ._binding import language

if TYPE_CHECKING:
    HIGHLIGHTS_QUERY: str
    INJECTIONS_QUERY: str
    LOCALS_QUERY: str
    TAGS_QUERY: str


def _get_query(file: str) -> str:
    query = _files(f"{__package__}") / file
    return query.read_text()


def __getattr__(name: str) -> str:
    if name == "HIGHLIGHTS_QUERY":
        query = _get_query("queries/highlights.scm")
        globals()[name] = query
        return query
    if name == "INJECTIONS_QUERY":
        query = _get_query("queries/injections.scm")
        globals()[name] = query
        return query
    if name == "LOCALS_QUERY":
        query = _get_query("queries/locals.scm")
        globals()[name] = query
        return query
    if name == "TAGS_QUERY":
        query = _get_query("queries/tags.scm")
        globals()[name] = query
        return query

    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


__all__ = [
    "HIGHLIGHTS_QUERY",
    "INJECTIONS_QUERY",
    "LOCALS_QUERY",
    "TAGS_QUERY",
    "language",
]


def __dir__() -> list[str]:
    return sorted(
        __all__
        + [
            "__all__",
            "__builtins__",
            "__cached__",
            "__doc__",
            "__file__",
            "__loader__",
            "__name__",
            "__package__",
            "__path__",
            "__spec__",
        ]
    )
