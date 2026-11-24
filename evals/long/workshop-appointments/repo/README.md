# Taller Martínez

Sistema del mostrador del taller: clientes y sus vehículos, mecánicos, servicios,
repuestos con stock, órdenes de trabajo, presupuestos y reportes. Solo biblioteca
estándar de Python 3.10+.

Los datos viven en un archivo JSON (por defecto `samples/taller.json`). Todo se usa a
través de `taller.shop.Shop`:

```python
from taller.shop import Shop
shop = Shop.load("samples/taller.json")
order = shop.open_order("AB123CD", ["OIL"])
shop.add_part(order.id, "FIL-ACE", 1)
shop.close_order(order.id, "2025-03-10")
shop.save()
```

## Módulos

- `models.py`: los datos (clientes, vehículos, mecánicos, servicios, repuestos, órdenes).
- `storage.py`: lee y escribe el JSON.
- `customers.py`, `staff.py`, `catalog.py`, `inventory.py`: cada parte del taller.
- `orders.py`: órdenes de trabajo; `quotes.py`: presupuestos.
- `messages.py`: los mensajes que se mandan a los clientes (quedan en `shop.outbox`).
- `reports.py`: reportes en texto; `money.py`: importes en centavos.
- `validation.py`: patentes, teléfonos y correos.
- `cli.py`: la línea de comandos.

## Uso

```sh
python3 -m taller clientes
python3 -m taller vehiculos
python3 -m taller mecanicos
python3 -m taller servicios
python3 -m taller repuestos [--bajo-stock]
python3 -m taller presupuesto AB123CD OIL BRK
python3 -m taller ordenes [--abiertas]
python3 -m taller reporte 2025-03
```

Todas aceptan `--data <archivo>`.

## Tests

```sh
python3 -m unittest discover -s tests -t .
```

Los montos van siempre en centavos enteros.
