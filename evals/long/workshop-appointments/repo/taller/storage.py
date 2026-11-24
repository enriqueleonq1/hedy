"""Lee y escribe los datos del taller en un archivo JSON."""
import json
from pathlib import Path

from .models import Customer, Mechanic, Part, PartUse, Service, Vehicle, WorkOrder

DEFAULT_PATH = Path(__file__).resolve().parent.parent / "samples" / "taller.json"


def read(path=DEFAULT_PATH) -> dict:
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def write(data: dict, path) -> None:
    path = Path(path)
    tmp = path.with_suffix(path.suffix + ".tmp")
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=2)
        f.write("\n")
    tmp.replace(path)


def _uses(items) -> list[PartUse]:
    return [PartUse(i["code"], int(i["qty"])) for i in items or []]


def customers_from(data: dict) -> list[Customer]:
    return [Customer(c["id"], c["name"], c.get("phone", ""), c.get("email", "")) for c in data.get("customers", [])]


def vehicles_from(data: dict) -> list[Vehicle]:
    return [Vehicle(v["plate"], v["customer"], v["brand"], v["model"], int(v["year"])) for v in data.get("vehicles", [])]


def mechanics_from(data: dict) -> list[Mechanic]:
    return [Mechanic(m["id"], m["name"], list(m.get("skills", []))) for m in data.get("mechanics", [])]


def services_from(data: dict) -> list[Service]:
    return [
        Service(s["code"], s["name"], int(s["minutes"]), s["skill"], int(s["labor_cents"]), _uses(s.get("parts")))
        for s in data.get("services", [])
    ]


def parts_from(data: dict) -> list[Part]:
    return [
        Part(p["code"], p["name"], int(p["price_cents"]), int(p["stock"]), int(p.get("min_stock", 0)))
        for p in data.get("parts", [])
    ]


def orders_from(data: dict) -> list[WorkOrder]:
    return [
        WorkOrder(
            int(o["id"]), o["plate"], o.get("mechanic"), list(o["services"]), _uses(o.get("parts")),
            o["opened"], o.get("closed"), o.get("status", "abierta"), int(o.get("total_cents", 0)),
        )
        for o in data.get("orders", [])
    ]


def _uses_dict(uses: list[PartUse]) -> list[dict]:
    return [{"code": u.code, "qty": u.qty} for u in uses]


def customers_to(items: list[Customer]) -> list[dict]:
    return [{"id": c.id, "name": c.name, "phone": c.phone, "email": c.email} for c in items]


def vehicles_to(items: list[Vehicle]) -> list[dict]:
    return [{"plate": v.plate, "customer": v.customer, "brand": v.brand, "model": v.model, "year": v.year} for v in items]


def mechanics_to(items: list[Mechanic]) -> list[dict]:
    return [{"id": m.id, "name": m.name, "skills": list(m.skills)} for m in items]


def services_to(items: list[Service]) -> list[dict]:
    return [
        {"code": s.code, "name": s.name, "minutes": s.minutes, "skill": s.skill,
         "labor_cents": s.labor_cents, "parts": _uses_dict(s.parts)}
        for s in items
    ]


def parts_to(items: list[Part]) -> list[dict]:
    return [
        {"code": p.code, "name": p.name, "price_cents": p.price_cents, "stock": p.stock, "min_stock": p.min_stock}
        for p in items
    ]


def orders_to(items: list[WorkOrder]) -> list[dict]:
    return [
        {"id": o.id, "plate": o.plate, "mechanic": o.mechanic, "services": list(o.services),
         "parts": _uses_dict(o.parts), "opened": o.opened, "closed": o.closed,
         "status": o.status, "total_cents": o.total_cents}
        for o in items
    ]
