# Arquitectura

Todo corre en memoria: `storage/csv_store.py` arma un `Library` (`library/state.py`)
con el catálogo, los socios, los préstamos, las reservas y el libro de multas, y
cada operación lo modifica.

```
cli.py ──► reports/*      ──► fines/calculator.py ──► loans/due.py ──► calendar/
       └─► catalog/search                         └─► members/tiers.py
loans/returns.py ──► fines/calculator.py
                 └─► holds/allocation.py ──► holds/priority.py, members/standing.py
```

## Decisiones

- Los montos se guardan en centavos enteros (`int`); `money.fmt_cents` los
  formatea solo al mostrarlos.
- Las fechas son `datetime.date`; nunca se usa la fecha del sistema dentro de la
  lógica: quien llama pasa el día (`on`).
- El calendario de cierres es por sucursal: `calendar.loader.load_calendar(branch)`.
- `library/vendor/` se copia de proyectos externos y no se modifica.
- `data/catalog_snapshot.json` es generado por `scripts/export_catalog.py`; no se
  edita a mano.
