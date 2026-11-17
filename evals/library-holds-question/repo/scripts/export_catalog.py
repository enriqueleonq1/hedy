"""Writes data/catalog_snapshot.json from the cataloguing system's export.

Run nightly by the server's scheduler:

    python3 scripts/export_catalog.py exports/catalogo.tsv > data/catalog_snapshot.json

The TSV has one row per copy: title id, title, author, year, subjects (comma
separated), item id, kind, branch, barcode, acquisition date.
"""
import csv
import json
import sys


def main(path: str) -> None:
    titles: dict[str, dict] = {}
    generated = ""
    with open(path, encoding="utf-8", newline="") as f:
        for row in csv.reader(f, delimiter="\t"):
            if row and row[0].startswith("#generated"):
                generated = row[0].split("=", 1)[1]
                continue
            tid, title, author, year, subjects, item, kind, branch, barcode, acquired = row
            entry = titles.setdefault(
                tid,
                {
                    "id": tid,
                    "title": title,
                    "author": author,
                    "year": int(year),
                    "subjects": [s.strip() for s in subjects.split(",") if s.strip()],
                    "copies": [],
                },
            )
            entry["copies"].append(
                {"item": item, "kind": kind, "branch": branch, "barcode": barcode, "acquired": acquired}
            )
    out = {
        "generated": generated,
        "source": "catalogador nocturno",
        "count": len(titles),
        "titles": sorted(titles.values(), key=lambda t: t["id"]),
    }
    json.dump(out, sys.stdout, ensure_ascii=False, indent=2)
    print()


if __name__ == "__main__":
    main(sys.argv[1])
