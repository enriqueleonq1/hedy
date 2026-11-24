"""Alta de muchos socios desde un CSV de otra fuente (la planilla de la
recepción). Cada fila se valida como un alta normal; las que fallan se
informan sin cortar el resto."""
import csv
from dataclasses import dataclass, field

from .members import add_member
from .models import GymError

REQUIRED = ["id", "name", "email", "plan"]


@dataclass
class ImportReport:
    added: list = field(default_factory=list)
    skipped: list = field(default_factory=list)  # [(número de fila, motivo)]

    def text(self):
        rows = [f"Altas: {len(self.added)}", f"Rechazadas: {len(self.skipped)}"]
        for line, why in self.skipped:
            rows.append(f"  fila {line}: {why}")
        return "\n".join(rows)


def missing_columns(fieldnames):
    return [c for c in REQUIRED if c not in (fieldnames or [])]


def import_members(state, path):
    """Da de alta los socios del CSV en `path` y devuelve el informe."""
    report = ImportReport()
    with open(path, encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        missing = missing_columns(reader.fieldnames)
        if missing:
            raise GymError(f"Faltan columnas: {', '.join(missing)}")
        for number, row in enumerate(reader, start=2):
            try:
                member = add_member(
                    state,
                    row["id"],
                    row["name"],
                    row["email"],
                    row["plan"].strip().lower(),
                    row.get("family", "") or "",
                )
            except GymError as error:
                report.skipped.append((number, str(error)))
                continue
            report.added.append(member.id)
    return report
