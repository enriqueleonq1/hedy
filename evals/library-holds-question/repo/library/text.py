"""Text helpers shared by search and reports."""
import unicodedata


def fold(text: str) -> str:
    """Lowercase without accents: "Cortázar" -> "cortazar"."""
    decomposed = unicodedata.normalize("NFKD", text)
    return "".join(c for c in decomposed if not unicodedata.combining(c)).lower()


def words(text: str) -> list[str]:
    """Folded words, punctuation dropped."""
    cleaned = "".join(c if c.isalnum() else " " for c in fold(text))
    return cleaned.split()


def shorten(text: str, width: int) -> str:
    """Cuts `text` to `width` characters, ending in "…" when cut."""
    if len(text) <= width:
        return text
    return text[: max(width - 1, 0)] + "…"
