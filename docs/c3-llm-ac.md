# C3 — LLM preentrenado + codificación aritmética (techo teórico)

**Meta:** medir el techo real del enfoque DeepMind ("language modeling is compression") sobre el
corpus real de Cristian con lo que tenemos (CPU, sin GPU), y decidir con números si merece
tier opt-in. La cota de compresión de un LM es su cross-entropy: bpb = NLL_bits/byte —
medible con llama-perplexity SIN construir el coder.

Plan:
1. F0 (hoy): build CPU de llama.cpp (fuente ya presente en el server, checkout verticemind) +
   modelo pequeño GGUF (0.5-1.6B, Q8) → llama-perplexity sobre muestras reales (código/sql/logs)
   → tabla bpb-cota vs paq8px/zpaq + tok/s medidos.
2. Si la cota gana con margen: construir el coder AC sobre logits (float → NO determinista
   cross-arch: SOLO modo "mismo binario mismo hardware", etiquetado así) o esperar C2-style
   cuantización de logits (int) para el modo portable.
3. GPU alquilada para modelos grandes = gasto → decisión del dueño (no se asume).
4. El balance SIEMPRE cuenta el modelo amortizado (ej.: modelo 600 MB sobre corpus 2,4 GB = +25
   puntos de ratio si se cuenta entero → solo tiene sentido como servicio/instalación compartida,
   nunca "por archivo"). Este número va en toda tabla que publique C3.
