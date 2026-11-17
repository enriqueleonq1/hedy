# Biblioteca municipal

Préstamos, reservas y multas de las tres sucursales de la biblioteca municipal
(Centro, Norte y Puerto). Lo usa el mostrador y genera los reportes mensuales
que se mandan a la comisión directiva.

## Uso

```sh
python3 -m library report fines 2025-03        # multas del mes
python3 -m library report circulation 2025-03  # préstamos por sucursal
python3 -m library report overdue 2025-06-30   # vencidos a una fecha
python3 -m library search "cortazar"
python3 -m library member M001
```

Los datos salen de `fixtures/` (exportación del sistema del mostrador) y de `data/`
(feriados, sucursales y el catálogo exportado cada noche).

## Tests

```sh
python3 -m unittest discover -s tests -t .
```

Solo biblioteca estándar de Python 3.10+.

## Estructura

- `library/policy/rules.py`: plazos, límites y montos de la política de préstamos.
- `library/calendar/`: días en que cada sucursal está cerrada.
- `library/catalog/`: catálogo (lee `data/catalog_snapshot.json`, que es generado).
- `library/members/`: socios, categorías y si están habilitados.
- `library/loans/`: préstamos, vencimientos, devoluciones y renovaciones.
- `library/fines/`: cálculo de multas y libro de multas.
- `library/holds/`: reservas.
- `library/notifications/`: textos que se mandan a los socios.
- `library/reports/`: reportes mensuales.
- `library/vendor/`: código de terceros copiado tal cual; no se edita.

Más detalle en `docs/`.
