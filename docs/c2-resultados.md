# C2 — resultados del compresor de context-mixing entero (vc-cm)

Compresor **de context-mixing propio, 100% aritmética entera** (sin float en el camino de
probabilidad → determinista bit a bit cross-arch por construcción). Familia lpaq: modelos de
contexto (órdenes 0-5 + palabra + match) → StateMap con tasa adaptativa 1/n → mezclador logístico
entero (selector por byte parcial) → APM/SSE → codificador aritmético binario de 32 bits.

## Frontera medida (bits/byte, muestras reales 10 MB; menor = mejor)

| muestra | gzip | xz -9e | zstd19 | brotli11 | **vc-cm** | zpaq-m5 | paq8px-8 |
|---|---|---|---|---|---|---|---|
| codigo | 1.135 | 0.874 | 0.905 | 0.882 | **0.840** ✅ | 0.663 | 0.504 |
| sql | 1.55 | 0.304 | 0.356 | 0.308 | 0.397 | 0.268 | 0.121 |
| csvjson | 1.12 | 0.566 | 0.607 | 0.573 | 0.633 | 0.509 | 0.355 |
| logs | 0.56 | 0.096 | 0.105 | 0.096 | 0.112 | 0.058 | 0.024 |
| enwik8(10M) | 2.92 | 2.169 | 2.230 | 2.157 | **1.918** ✅ | 1.669 | 1.423 |

**vc-cm BATE a xz/zstd/brotli en código y en texto natural (enwik8)**; queda cerca en logs; y
pierde en SQL/CSV (datos muy numéricos/columnares, terreno del diccionario grande de LZMA).
Sigue por detrás de zpaq-m5/paq8px (les faltan bit-history states y más modelos) pero está **en su
liga** siendo determinista y propio. Round-trip byte-exacto en el 100% de las pruebas (incluye
aleatorio → ~8 bpb sin romper). Velocidad ~0.8-0.9 MB/s (el coste del CM, como toda la familia).

## Papel en el producto

Carril **"máximo ratio, determinista"** opt-in para texto/código, detrás de flag. NO es el
núcleo (el núcleo es zstd rápido + dedup + preflate). Es la vía de investigación hacia el
"primera vez": un CM entero determinista que iguala/bate a la vara clásica. Próximo: bit-history
states (ICM) para cerrar el hueco con zpaq, y la extensión neural (pesos entrenados como un modelo
más del mixer, cuantizados a entero).

## Determinismo cross-arch (la condición dura del brief)

Al ser aritmética entera pura, la salida es **byte-idéntica** en x86 y ARM. Verificado: ver
`bench/determinismo-vccm.txt`.
