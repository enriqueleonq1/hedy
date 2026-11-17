"""Reads the catalog snapshot exported every night by the cataloguing system.

data/catalog_snapshot.json is generated (scripts/export_catalog.py); never edit it
by hand.
"""
import json
from dataclasses import dataclass
from functools import lru_cache

from library.config import CATALOG_SNAPSHOT_FILE

from .models import Item, Title


@dataclass
class Catalog:
    titles: dict[str, Title]
    items: dict[str, Item]

    def title_of(self, item_id: str) -> Title:
        return self.titles[self.items[item_id].title_id]

    def copies(self, title_id: str) -> list[Item]:
        return [self.items[i] for i in self.titles[title_id].item_ids]


def parse_snapshot(data: dict) -> Catalog:
    titles: dict[str, Title] = {}
    items: dict[str, Item] = {}
    for entry in data["titles"]:
        title = Title(
            id=entry["id"],
            title=entry["title"],
            author=entry["author"],
            year=int(entry["year"]),
            subjects=list(entry.get("subjects", [])),
        )
        for copy in entry.get("copies", []):
            item = Item(id=copy["item"], title_id=title.id, kind=copy["kind"], branch=copy["branch"])
            items[item.id] = item
            title.item_ids.append(item.id)
        titles[title.id] = title
    return Catalog(titles=titles, items=items)


@lru_cache(maxsize=1)
def load_snapshot(path=CATALOG_SNAPSHOT_FILE) -> Catalog:
    with open(path, encoding="utf-8") as f:
        return parse_snapshot(json.load(f))
