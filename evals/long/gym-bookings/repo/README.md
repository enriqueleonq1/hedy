# Gimnasio Núcleo

Clases, socios, reservas y cuotas del gimnasio. Todo vive en una carpeta de datos
(`data/` por defecto):

- `classes.json`: las clases (id, nombre, inicio, capacidad, instructor, sala, minutos);
- `members.csv`: los socios (id, nombre, email, plan `basic` o `premium`, familia);
- `state.json`: reservas, pagos y mensajes (lo escribe el programa).

`gym/app.py` (`Gym`) junta todo; la CLI (`gym/cli.py`) lo usa:

```sh
python3 -m gym clases
python3 -m gym socios
python3 -m gym --ahora 2025-03-10T09:00 reservar s001 yoga-1003
python3 -m gym factura s001 2025-03
python3 -m gym reporte ocupacion 2025-03-11
python3 -m gym horario 2025-03-10
python3 -m gym revisar
python3 -m unittest discover -s tests -t .
```

Los montos van en centavos enteros; el IVA es el 21 %. Los mensajes a los socios se
dejan en la bandeja de salida (`state.json`) y otro proceso los envía.

Solo biblioteca estándar de Python 3.10+.
