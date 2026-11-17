"""Where the library keeps its data files."""
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA_DIR = ROOT / "data"
FIXTURES_DIR = ROOT / "fixtures"

HOLIDAYS_FILE = DATA_DIR / "holidays.json"
BRANCHES_FILE = DATA_DIR / "branches.json"
CATALOG_SNAPSHOT_FILE = DATA_DIR / "catalog_snapshot.json"

MEMBERS_FILE = FIXTURES_DIR / "members.csv"
LOANS_FILE = FIXTURES_DIR / "loans.csv"
HOLDS_FILE = FIXTURES_DIR / "holds.csv"
FINES_FILE = FIXTURES_DIR / "fines.csv"

CURRENCY_SYMBOL = "$"
DATE_FORMAT = "%d/%m/%Y"
