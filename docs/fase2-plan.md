# Fase 2 — plan aprobado en el gate (2026-07-25)

Decisiones del dueño: **F2 OK** · umbral: **≥15 MB/s/núcleo** compresión normal, **≥400 MB/s** descompresión · **C1+C2+C3 en paralelo**.

Orden ROI (del informe F1 §12):
1. **F2.1 núcleo montable** (este repo, Rust): contenedor CAS + dedup CDC (FastCDC) + router por carriles + frames seekables (2 MiB) + zstd/store + CLI `comprimir/descomprimir/verificar/info`. Regla dura: correctitud > ratio > velocidad; blake3 por fichero y por índice; fallback store SIEMPRE.
2. **F2.2 preflate-rs** (palanca nº1: +26-45% medido): carril DEFLATE (gz/zip/docx/pdf/png) con corpus adversario multi-herramienta y byte-exacto o store.
3. **F2.3 JXL** (+25% medido, 238/241): carril JPEG con reconstrucción verificada; fallo → store.
4. **F2.4 FUSE RO** (`montar`): bloque 1-4 MiB (curva medida en F1 §5), descompresión zstd (963 MB/s medidos).
5. **F2.5 suite**: fuzz round-trip ≥1M, no-expansión (urandom/mp4), benchmark de terceros con comando único (vs zip/zstd/xz/7z/DwarFS).

Etapa C en paralelo (docs/c1-zpaql.md · c2-red-entera.md · c3-llm-ac.md). Los números de referencia
viven en el framework: `VerticeLabsAgents/clients/vcompresor/{informe-fase1.md,bench/}`.
