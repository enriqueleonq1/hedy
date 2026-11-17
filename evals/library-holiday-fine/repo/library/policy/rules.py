"""Lending policy in one place.

Amounts are integer cents and periods are days. The board approves changes to
these values; see docs/loan-policy.md before editing them.
"""

# Loan period by item kind, in calendar days.
LOAN_DAYS = {
    "book": 14,
    "dvd": 7,
    "magazine": 7,
    "audiobook": 21,
}

# Extra days on top of LOAN_DAYS, by member tier.
TIER_EXTRA_DAYS = {
    "adult": 0,
    "student": 7,
    "senior": 7,
    "staff": 14,
}

# How many items a member may have out at the same time.
MAX_ACTIVE_LOANS = {
    "adult": 5,
    "student": 8,
    "senior": 5,
    "staff": 15,
}

MAX_RENEWALS = 2

# Fines are charged per day the library was open after the due date.
FINE_PER_OPEN_DAY_CENTS = {
    "book": 25,
    "dvd": 100,
    "magazine": 10,
    "audiobook": 25,
}
MAX_FINE_PER_LOAN_CENTS = 1000

# Tiers that never pay fines.
FINE_EXEMPT_TIERS = frozenset({"staff"})

# Members owing more than this cannot borrow nor receive a held copy.
MAX_UNPAID_FINES_CENTS = 500

# Open days a member has to pick up a held copy once it is ready.
HOLD_PICKUP_DAYS = 3

# A returned copy goes first to holds picked up at the branch where it was
# returned, unless an older hold has been waiting longer than this.
LOCAL_PREFERENCE_DAYS = 7

MAX_HOLDS_PER_MEMBER = 5
