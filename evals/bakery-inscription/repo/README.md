# Panadería La Espiga

Pedidos por encargo del mostrador. Cada pedido es un archivo `.pedido` en la
carpeta del día (ver `samples/`):

```
cliente: Marta Ruiz
fecha: 2025-03-14
hora: 09:30
nota: sin azúcar glas
---
2 x baguette
1 x tarta de manzana
```

Campos opcionales del encabezado: `nota`, `domicilio` (agrega el envío),
`caja de regalo: sí|no` y `pagado: sí|no`.

## Uso

```sh
python3 -m bakery list samples                  # pedidos del día
python3 -m bakery list samples --solo-domicilio
python3 -m bakery ticket samples/2025-03-14-001.pedido
python3 -m bakery csv samples > contabilidad.csv
python3 -m bakery check samples
```

## Tests

```sh
python3 -m unittest discover -s tests -t .
```

Solo biblioteca estándar de Python 3.10+. Los montos van en centavos enteros.
