INVOICES = {
    "1001": {"customer": "Acme", "total": "120.00"},
    "1002": {"customer": "Globex", "total": "75.50"},
}


def find_invoice(invoice_id):
    return INVOICES.get(invoice_id)
