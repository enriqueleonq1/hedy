from dataclasses import dataclass, field

KINDS = ("book", "dvd", "magazine", "audiobook")


@dataclass(frozen=True)
class Item:
    """One physical copy that can be lent."""

    id: str
    title_id: str
    kind: str
    branch: str

    def __post_init__(self):
        if self.kind not in KINDS:
            raise ValueError(f"unknown item kind: {self.kind}")


@dataclass
class Title:
    """A work in the catalog, with all its copies."""

    id: str
    title: str
    author: str
    year: int
    subjects: list[str] = field(default_factory=list)
    item_ids: list[str] = field(default_factory=list)

    def label(self) -> str:
        return f"{self.title} ({self.author}, {self.year})"
