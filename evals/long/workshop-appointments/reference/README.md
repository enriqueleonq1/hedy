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

## Turnos

Los turnos se dan desde el mostrador y siguen hasta la orden de trabajo:

```python
from datetime import date
a = shop.book("AB123CD", "OIL", date(2025, 3, 10), "09:00")   # reserva sus repuestos
shop.reminder(a.id)              # el texto que recibe el cliente
shop.free_slots(date(2025, 3, 10), "ALI")
order = shop.start_order(a.id)   # el auto llegó
shop.close_order(order.id, "2025-03-10")   # el turno queda terminado
```

- Horario: lunes a viernes de 08:00 a 18:00, con pausa de almuerzo; los turnos empiezan
  cada 30 minutos.
- Cada servicio lo hace el primer mecánico libre con su especialidad.
- Lo reservado para turnos no cuenta como disponible (`shop.inventory.available`).
- Los errores de un turno son `BookingError`, con el mensaje para el mostrador.

```sh
python3 -m taller turno AB123CD OIL 2025-03-10 09:00
python3 -m taller agenda 2025-03-10
python3 -m taller cancelar 1
```

## Módulos

- `models.py`: los datos (clientes, vehículos, mecánicos, servicios, repuestos, órdenes, turnos).
- `agenda.py`: el horario del taller, las franjas y qué mecánico atiende cada turno.
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
