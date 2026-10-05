# preflate — vendorizado

**Origen:** `preflate-rs` 0.7.6 · https://github.com/microsoft/preflate-rs
**Autor:** Kristof Roomp · Copyright (c) Microsoft Corporation
**Licencia:** Apache-2.0 — el texto completo está en `LICENSE`, en este mismo directorio.

## Por qué está aquí

**No por poseerlo.** Por una mejora que está medida y que desde fuera no se puede coger:

> preflate re-deflata el plano **entero** solo para verificarse: **41,58 s de sus 172,97, el 24%**
> de su tiempo. Y preflate es el **59,24% de la CPU de transformada** del archivador.

Su premisa era *«soy una herramienta suelta, no hay nadie más comprobando»*. Aquí sí lo hay:
`vcompresor verify` revisa el archivo completo después. **Verificamos dos veces.**

Y no se toca sin el número delante: **la prueba que abre ese cambio es contar cuántos de los
1.973 flujos del banco rechaza esa verificación.** Si rechaza cero, hablamos. Si rechaza alguno,
se queda — porque entonces está cazando algo real.

## Lo que esto NO arregla

- **No mejora el 1,700%.** Ya estamos ahí, porque ya lo usábamos. Lo que se gana es poder cambiarlo.
- **No quita el árbol de dependencias**: arrastra `bitcode`, `byteorder`, `cabac`, `default-boxed`
  y `deranged`. Y `cabac` es el codificador aritmético — el mecanismo que hace que su 1,700% sea
  1,700%. Ese sigue siendo de fuera.
- **No cambia lo que midió la fase 3**: nuestra marca sola cuesta 4,618% contra su 1,700% *total*.
  Lo que falla es el **estimador**, no el codificador.

## Cambios respecto al original

| fecha | fichero | qué |
|---|---|---|
| 2026-07-31 | `Cargo.toml` | reescrito para el workspace. El paquete pasa a llamarse `preflate`; **la lib sigue siendo `preflate_rs`** para que los `use` de vc-core no cambien y el traslado no se mezcle con ningún cambio de comportamiento. |

**Ningún fichero `.rs` modificado todavía.** Cuando se toque uno, lleva cabecera diciéndolo —es la
obligación §4(b) de Apache-2.0— y una fila en esta tabla.

## Cómo se comprueba que el traslado no cambió nada

Los bytes de salida sobre el banco de 1.973 flujos tienen que ser **idénticos** antes y después.
Un traslado que cambia un byte no es un traslado: es un cambio sin medir.
