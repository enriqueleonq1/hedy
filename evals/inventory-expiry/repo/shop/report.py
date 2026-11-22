def pending_report(inventory):
    """One line per pending (unconfirmed) reservation, oldest first."""
    lines = []
    pending = [r for r in inventory.reservations() if not r.confirmed]
    for r in sorted(pending, key=lambda r: r.created_at):
        name = inventory.product(r.sku).name
        lines.append(f"#{r.id} {name} x{r.qty}")
    return "\n".join(lines)
