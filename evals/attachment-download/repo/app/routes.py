"""A tiny router: handle(path) -> (status, body).

The path arrives as the client sent it; it is percent-decoded here before
matching, so `/invoices/10%30%31` is `/invoices/1001`.
"""

from urllib.parse import unquote

from .invoices import find_invoice


def health(_rest):
    return 200, "ok"


def invoice(invoice_id):
    found = find_invoice(invoice_id)
    if found is None:
        return 404, "not found"
    return 200, f"{found['customer']}: {found['total']}"


ROUTES = {
    "/health": health,
    "/invoices/": invoice,
}


def handle(raw_path):
    path = unquote(raw_path)
    for prefix, view in ROUTES.items():
        if prefix.endswith("/") and path.startswith(prefix):
            return view(path[len(prefix):])
        if path == prefix:
            return view("")
    return 404, "not found"
