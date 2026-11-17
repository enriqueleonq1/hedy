"""What each membership tier allows."""
from library.policy.rules import FINE_EXEMPT_TIERS, MAX_ACTIVE_LOANS, TIER_EXTRA_DAYS

LABELS = {
    "adult": "Adulto",
    "student": "Estudiante",
    "senior": "Mayor de 65",
    "staff": "Personal",
}


def tier_label(tier: str) -> str:
    return LABELS.get(tier, tier)


def loan_limit(tier: str) -> int:
    return MAX_ACTIVE_LOANS[tier]


def extra_days(tier: str) -> int:
    return TIER_EXTRA_DAYS.get(tier, 0)


def pays_fines(tier: str) -> bool:
    return tier not in FINE_EXEMPT_TIERS
