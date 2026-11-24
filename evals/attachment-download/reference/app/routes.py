"""A tiny router: handle(path) -> (status, body).

The path arrives as the client sent it; it is percent-decoded here before
matching, so `/invoices/10%30%31` is `/invoices/1001`.
"""

from urllib.parse import unquote

from . import config
from .invoices import find_invoice


def health(_rest):
    return 200, "ok"


def invoice(invoice_id):
    found = find_invoice(invoice_id)
    if found is None:
        return 404, "not found"
    return 200, f"{found['customer']}: {found['total']}"


def attachment(name):
    root = config.UPLOAD_DIR.resolve()
    target = (root / name).resolve()
    if target.parent != root or not target.is_file():
        return 404, "not found"
    return 200, target.read_text()


ROUTES = {
    "/health": health,
    "/invoices/": invoice,
    "/attachments/": attachment,
}


def handle(raw_path):
    path = unquote(raw_path)
    for prefix, view in ROUTES.items():
        if prefix.endswith("/") and path.startswith(prefix):
            return view(path[len(prefix):])
        if path == prefix:
            return view("")
    return 404, "not found"
