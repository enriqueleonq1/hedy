"""Catalog search by title, author or subject, ignoring case and accents."""
from library.text import fold, words

from .models import Title
from .snapshot import Catalog


def _haystack(title: Title) -> str:
    return " ".join([fold(title.title), fold(title.author), *map(fold, title.subjects)])


def search(catalog: Catalog, query: str, limit: int = 20) -> list[Title]:
    """Titles whose title, author or subjects contain every word of `query`.

    Exact title matches come first, then by title.
    """
    wanted = words(query)
    if not wanted:
        return []
    found = [t for t in catalog.titles.values() if all(w in _haystack(t) for w in wanted)]
    exact = fold(query.strip())
    found.sort(key=lambda t: (fold(t.title) != exact, fold(t.title), t.id))
    return found[:limit]


def by_author(catalog: Catalog, author: str) -> list[Title]:
    wanted = fold(author)
    return sorted(
        (t for t in catalog.titles.values() if wanted in fold(t.author)),
        key=lambda t: (t.year, fold(t.title)),
    )


def available_copies(catalog: Catalog, title_id: str, lent_item_ids: set[str], branch: str | None = None):
    """Copies of a title that are on the shelf, optionally at one branch."""
    return [
        item
        for item in catalog.copies(title_id)
        if item.id not in lent_item_ids and (branch is None or item.branch == branch)
    ]
