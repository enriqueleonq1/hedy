from dataclasses import dataclass

TIERS = ("adult", "student", "senior", "staff")


@dataclass(frozen=True)
class Member:
    id: str
    name: str
    tier: str
    home_branch: str
    email: str = ""
    suspended: bool = False

    def __post_init__(self):
        if self.tier not in TIERS:
            raise ValueError(f"unknown tier: {self.tier}")
