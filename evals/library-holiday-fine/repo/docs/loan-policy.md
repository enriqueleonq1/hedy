# Política de préstamos

Aprobada por la comisión directiva (acta 112, diciembre de 2024). Los valores
viven en `library/policy/rules.py`.

## Plazos

| Material    | Días |
|-------------|------|
| Libro       | 14   |
| DVD         | 7    |
| Revista     | 7    |
| Audiolibro  | 21   |

Estudiantes y mayores de 65 suman 7 días; el personal suma 14.

Cada renovación agrega un plazo completo. Se puede renovar hasta dos veces si
nadie está esperando el título y el préstamo no está vencido.

## Vencimientos y días cerrados

Las sucursales cierran los domingos y los feriados de `data/holidays.json`.
Algunos feriados son locales y solo cierran la sucursal indicada (por ejemplo
San Juan en Norte).

Si el vencimiento cae en un día en que la sucursal del préstamo está cerrada,
se corre al siguiente día abierto. Nadie paga multa por un día en que no podía
devolver el material.

## Multas

Se cobra por cada día **abierto** de la sucursal entre el vencimiento y la
devolución: 25 centavos por libro o audiolibro, 10 por revista y 1 peso por
DVD. Tope de 10 pesos por préstamo. El personal no paga multas.

Quien debe más de 5 pesos no puede retirar material ni recibir reservas hasta
pagar.

## Reservas

Un socio puede tener hasta 5 reservas activas. Cuando se devuelve un ejemplar,
se aparta para una de las reservas en espera y el socio tiene 3 días abiertos
para retirarlo. Se intenta evitar traslados entre sucursales.
