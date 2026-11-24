# Coworking La Nave

Herramientas del mostrador del coworking: socios, planes, salas de reuniones,
consumos del bar, control de acceso y facturas mensuales.

```sh
python3 -m cowork socios
python3 -m cowork planes
python3 -m cowork salas 2025-03-10
python3 -m cowork alta "Elena Mora" elena@example.com 33333333P flex 2025-03-01
python3 -m cowork reservar S001 azul 2025-03-10 10:00 11:30
python3 -m cowork consumo S001 cafe 3 2025-03-10
python3 -m cowork entrar S001 2025-03-10 09:05
python3 -m cowork facturar S001 2025-03
python3 -m cowork resumen 2025-03
```

Los datos viven en `data/` (otra carpeta con `--datos`): catálogos en JSON
(`plans.json`, `rooms.json`, `extras.json`, `members.json`) y lo que cambia
día a día en `state.json`.

## Código

- `cowork/models.py`: los datos. Los montos van siempre en centavos enteros.
- `cowork/storage.py`: carga y guarda la carpeta de datos (`Store`).
- `cowork/members.py`, `plans.py`, `rooms.py`, `extras.py`, `access.py`: cada área.
- `cowork/billing.py`: facturas del mes.
- `cowork/reports.py`: reportes para la gerencia.
- `cowork/messages.py`: correos a los socios (quedan en la bandeja de salida).
- `cowork/cli.py`: la línea de comandos.

## Tests

```sh
python3 -m unittest discover -s tests -t .
```

Solo biblioteca estándar de Python 3.10+.
