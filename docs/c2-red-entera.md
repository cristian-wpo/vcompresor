# C2 — red neuronal ENTERA propia (la apuesta "primera vez")

**Meta publicable:** compresor neural con inferencia 100% enteros (int16/int32 fixed-point,
sin floats en el camino de probabilidad) → bit-determinista cross-arch POR CONSTRUCCIÓN,
que en el corpus real bata a zpaq -m5 y se acerque a paq8px a ≥10× su velocidad.
El modelo se fija, se versiona y CUENTA amortizado en el balance (regla del brief).

Diseño v0:
- Modelo byte-level por carril (código/sql/csvjson/logs): GRU/LSTM pequeña (256-512 hidden,
  1-2 capas, ~1-4M params) o mixer de contextos aprendido (estilo lpaq pero pesos entrenados offline).
- Entrenamiento OFFLINE en float (PyTorch/CPU del server o Mac) sobre el corpus real →
  cuantización a int16 con escala fija por capa + tabla sigmoide/softmax entera precalculada.
- Inferencia: C/Rust puro enteros (mul+shift, sin libm); codificador aritmético binario entero
  (estilo carryless rc de zpaq, ya determinista).
- Test de aceptación = matriz F1 §9 ampliada: encode+decode en x86 y ARM, 4 cruces byte-exactos.
- Literatura de apoyo (verificada 24-07): integer-only inference pipelines para compresión
  neural reproducible; logits cuantizados a espacio discreto de probabilidad.

Hitos: H1 datos+tokenizador byte (ya: corpus F1) · H2 entrenar GRU float por carril y medir bpb
teórico (cross-entropy = cota de compresión) · H3 cuantizar y medir pérdida de bpb (float→int) ·
H4 coder aritmético entero + round-trip · H5 matriz cross-arch + velocidad · H6 integrar como
carril opt-in del contenedor.
