"""Whether a member is in good standing to borrow or receive held copies."""
from dataclasses import dataclass

from library.fines.ledger import FineLedger
from library.policy.rules import MAX_UNPAID_FINES_CENTS

from .models import Member
from .tiers import loan_limit


@dataclass(frozen=True)
class Standing:
    ok: bool
    reason: str = ""


def owes_too_much(member: Member, ledger: FineLedger) -> bool:
    return ledger.unpaid_cents(member.id) > MAX_UNPAID_FINES_CENTS


def standing(member: Member, ledger: FineLedger, active_loans: int = 0) -> Standing:
    """Suspension first, then debt, then the loan limit of the tier."""
    if member.suspended:
        return Standing(False, "suspended")
    if owes_too_much(member, ledger):
        return Standing(False, "unpaid_fines")
    if active_loans >= loan_limit(member.tier):
        return Standing(False, "loan_limit")
    return Standing(True)


def may_receive_holds(member: Member, ledger: FineLedger) -> bool:
    """Holds ignore the loan limit: the copy waits on the shelf for pickup."""
    return not member.suspended and not owes_too_much(member, ledger)
