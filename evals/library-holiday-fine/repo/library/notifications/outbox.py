"""Messages waiting to be sent by the nightly mail job."""
from dataclasses import dataclass, field

from library.loans.returns import ReturnOutcome
from library.state import Library

from . import messages


@dataclass(frozen=True)
class Message:
    to: str
    text: str


@dataclass
class Outbox:
    pending: list[Message] = field(default_factory=list)

    def add(self, to: str, text: str) -> None:
        if to:
            self.pending.append(Message(to, text))

    def drain(self) -> list[Message]:
        sent, self.pending = self.pending, []
        return sent


def after_return(outbox: Outbox, library: Library, outcome: ReturnOutcome) -> None:
    """Queues the fine notice and the "your hold is ready" message of a return."""
    loan = library.loans[outcome.loan_id]
    member = library.members[loan.member_id]
    title = library.catalog.title_of(loan.item_id).title
    if outcome.fine_cents:
        outbox.add(member.email, messages.fine_charged(member.name, title, outcome.fine_cents))
    hold = outcome.hold
    if hold is not None:
        waiting = library.members[hold.member_id]
        outbox.add(waiting.email, messages.hold_ready(waiting.name, title, hold.pickup_branch, hold.pickup_by))
