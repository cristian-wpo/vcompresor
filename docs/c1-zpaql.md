# C1 — CM especializado determinista (ZPAQL)

**Meta:** batir a zpaq -m5 genérico en las 4 categorías de texto del corpus real manteniendo
determinismo por especificación (VM de enteros ZPAQL, cross-arch DEMOSTRADO en F1 §9).
**Techo a perseguir (paq8px -8):** código 0,504 bpb · sql 0,121 · csvjson 0,355 · logs 0,024.
**Punto de partida (zpaq -m5):** 0,663 · 0,268 · 0,509 · 0,058.

Plan:
1. Sweep de métodos integrados (m1..m5 + variantes de bloque) sobre las muestras — mapa base (bench c1-sweep).
2. Leer los config .cfg de referencia del ecosistema zpaq (max.cfg/mid.cfg históricos) y el manual ZPAQL.
3. Configs propios por tipo: contextos de indentación/keywords (código PHP/Kotlin), columnas y
   delimitadores (CSV/SQL dumps), timestamps+niveles (logs). Un .cfg por carril, versionado aquí.
4. Gate por config: bpb < zpaq -m5 en su categoría, velocidad ≥ m5×0,5, round-trip + cross-arch x86↔ARM.
