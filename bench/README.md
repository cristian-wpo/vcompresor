# Benchmark reproducible

Un comando, corpus público, round-trip verificado en la misma tabla. Sin datos escondidos en el descompresor, sin medir solo lo que comprime bien.

```bash
bash bench/run.sh
```

Qué hace:
1. Compila `vcompresor` (`cargo build --release`).
2. Descarga el **corpus Silesia** (público, 211 MB, 12 ficheros) si no está.
3. Comprime con `zip -9`, `zstd -19`, `xz -9`, `7z -mx9` y `vcompresor`, midiendo **bytes + tiempo**.
4. Para `vcompresor` (y donde el formato lo permite) **verifica el round-trip**: extrae y compara el `sha256` del árbol completo. `rt=OK` significa byte-idéntico; cualquier `FAIL` invalida esa fila.
5. Imprime la tabla y la guarda en `bench/ultimo-resultado.tsv`.

Notas de honestidad (política del proyecto):
- El corpus es **fijo y público**: cualquiera reproduce la cifra.
- El tiempo y el ratio van juntos: un ratio bueno a velocidad inusable no es una victoria.
- `vcompresor` cae a **store** en lo incompresible: nunca engorda más que una cabecera pequeña (ver `vc-fuzz`, prueba de no-expansión).
- Para el corpus real privado del autor, ver `VerticeLabsAgents/clients/vcompresor/` (no reproducible por terceros por ser datos privados — por eso este bench usa Silesia).

---

# El banco: báscula, registro y panel

```bash
bench/mide.sh -e "qué estoy probando" -m sql.10m -c "VC_DECAY=16" -- \
    ./target/release/vcompresor comprimir /tmp/s -o /tmp/x.vc --max
python3 bench/panel.py        # -> bench/panel.html
```

**`mide.sh` es la báscula.** Toda medida pasa por ahí. Deja una línea JSON en
`bench/historia.jsonl` con el commit, si el árbol estaba sucio, la carga de la máquina,
los bytes, los segundos, la CPU, la memoria pico, los ciclos y el IPC. Append-only y
versionado: comparar contra "lo de anteayer" deja de costar volver a medirlo.

Por qué se anota la carga: **la misma prueba tarda 51,6 s o 97,1 s** según cómo esté la
máquina. Por eso el registro guarda reloj Y ciclos, y para comparar mandan los ciclos.

Por qué se anota el árbol sucio: una cifra de un árbol sin commitear no es reproducible,
y hay que saberlo al leerla, no al intentar repetirla tres semanas después.

**`panel.py` no calcula nada.** Lee `historia.jsonl`, `rivales.tsv`, `ablacion.tsv`,
`censo.tsv`, `perfil.tsv`, `poda.tsv` y el mapa de bits, y saca una página sola sin red
ni dependencias. Lo que falte sale marcado como pendiente, nunca inventado.

**`extrae_mapa.py`** convierte un volcado `VC_COSTEPOS` (un `f32` por byte = lo que costó
ese byte) en el mapa de calor. Alinea desde el FINAL: delante del volcado van los cuatro
tanteos de la sonda de perfil, y alinear desde el principio da mapas preciosos y falsos.
