"""Alta de clientes y vehículos desde un CSV (nombre,telefono,correo,patente,marca,modelo,año)."""
import csv
import io

from .errors import ShopError

COLUMNS = ["nombre", "telefono", "correo", "patente", "marca", "modelo", "año"]


def import_csv(shop, text: str) -> tuple[int, list[str]]:
    """Agrega lo que se puede; devuelve cuántas filas entraron y un error por fila que no."""
    reader = csv.DictReader(io.StringIO(text))
    missing = [c for c in COLUMNS if c not in (reader.fieldnames or [])]
    if missing:
        raise ShopError(f"Faltan columnas: {', '.join(missing)}")
    added, errors = 0, []
    by_name = {c.name.lower(): c for c in shop.customers.all()}
    for n, row in enumerate(reader, start=2):
        try:
            name = row["nombre"].strip()
            customer = by_name.get(name.lower())
            if customer is None:
                customer = shop.customers.add(name, row["telefono"], row["correo"])
                by_name[name.lower()] = customer
            if row["patente"].strip():
                shop.customers.add_vehicle(row["patente"], customer.id, row["marca"], row["modelo"],
                                           int(row["año"] or 0))
            added += 1
        except (ShopError, ValueError) as error:
            errors.append(f"fila {n}: {error}")
    return added, errors
