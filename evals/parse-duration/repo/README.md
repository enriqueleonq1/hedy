# Temporizador de pausas

El formulario de pausas recibe lo que escribe la gente a mano. `timer/` lo procesa.
Tests: `python3 -m unittest discover -s tests -t .`

## Formulario de pausas

La gente escribe la duración con horas `h` y minutos `m`, en cualquier combinación
(`1h30m`, `90m`, `2h`), a veces con espacios (`1h 30m`) o en mayúsculas (`1H30M`).
Lo que no se entiende (vacío, `abc`, `-5m`, `1x`) es un error de quien escribe:
se rechaza con `ValueError` para que el formulario lo marque, nunca se convierte en 0.
