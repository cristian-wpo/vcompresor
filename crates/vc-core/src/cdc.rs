// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! Troceado por contenido (CDC) estilo FastCDC con normalización. Propio: son ~120 líneas
//! y no justifica una dependencia.
//!
//! No necesita ser bit-compatible con ninguna otra implementación: el índice guarda los
//! offsets exactos de cada chunk, así que el troceador solo decide DÓNDE cortar. Lo único
//! que se juega aquí es la calidad del dedup, y eso se mide (ver bench/f2.7-deps.tsv).

use std::io::Read;

/// Tabla gear: 256 valores de 64 bits de un xorshift con semilla fija. Determinista.
const GEAR: [u64; 256] = gear();
const fn gear() -> [u64; 256] {
    let mut t = [0u64; 256];
    let mut x: u64 = 0x2545F491_4F6CDD1D;
    let mut i = 0;
    while i < 256 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        t[i] = x;
        i += 1;
    }
    t
}

pub struct Cortador {
    min: usize,
    avg: usize,
    pub max: usize,
    mask_s: u64,
    mask_l: u64,
}

impl Cortador {
    pub fn new(min: usize, avg: usize, max: usize) -> Self {
        // Normalización: antes de la media se exige una máscara MÁS difícil (más bits) y
        // después una MÁS fácil. Eso concentra los tamaños alrededor de la media en vez de
        // dejar la distribución exponencial del gear puro.
        let bits = (usize::BITS - avg.leading_zeros() - 1) as u64;
        // Los bits bajos del gear con desplazamiento a la izquierda los domina el último
        // byte; se mira una ventana más alta, que sí acumula historia.
        let masc = |b: u64| ((1u64 << b) - 1) << 15;
        Cortador { min, avg, max, mask_s: masc(bits + 2), mask_l: masc(bits.saturating_sub(2)) }
    }

    /// Longitud del primer chunk de `datos`.
    pub fn corte(&self, datos: &[u8]) -> usize {
        let n = datos.len();
        if n <= self.min {
            return n;
        }
        let fin = n.min(self.max);
        let normal = fin.min(self.avg);
        let mut fp: u64 = 0;
        let mut i = self.min;
        while i < normal {
            fp = (fp << 1).wrapping_add(GEAR[datos[i] as usize]);
            if fp & self.mask_s == 0 {
                return i + 1;
            }
            i += 1;
        }
        while i < fin {
            fp = (fp << 1).wrapping_add(GEAR[datos[i] as usize]);
            if fp & self.mask_l == 0 {
                return i + 1;
            }
            i += 1;
        }
        fin
    }
}

/// Trocea un slice en memoria. Devuelve (offset, longitud).
pub struct Trozos<'a> {
    datos: &'a [u8],
    pos: usize,
    c: Cortador,
}

pub fn trozos(datos: &[u8], min: usize, avg: usize, max: usize) -> Trozos<'_> {
    Trozos { datos, pos: 0, c: Cortador::new(min, avg, max) }
}

impl Iterator for Trozos<'_> {
    type Item = (usize, usize);
    fn next(&mut self) -> Option<(usize, usize)> {
        if self.pos >= self.datos.len() {
            return None;
        }
        let off = self.pos;
        let len = self.c.corte(&self.datos[off..]);
        self.pos += len;
        Some((off, len))
    }
}

/// Firma de similitud de un fichero: los K hashes más pequeños de sus trozos (minhash).
///
/// Por qué funciona: dos ficheros parecidos comparten trozos, y si comparten trozos comparten
/// sus mínimos. Ordenar por esta firma deja los parecidos JUNTOS, y como el compresor mira
/// hacia atrás dentro de su ventana, la redundancia ENTRE ficheros —que hasta ahora se perdía
/// entera— pasa a contar. Es la ganancia que zip no puede tener ni en principio, porque
/// comprime cada fichero aislado.
///
/// Se trocea con parámetros finos (~4 KiB) y sólo sobre una muestra acotada del principio:
/// suficiente para que los parecidos coincidan, y sin pagar una segunda pasada completa.
pub const SKETCH_K: usize = 4;
const SKETCH_MUESTRA: usize = 1024 * 1024;

pub fn sketch(datos: &[u8]) -> [u64; SKETCH_K] {
    let mut mins = [u64::MAX; SKETCH_K];
    let muestra = &datos[..datos.len().min(SKETCH_MUESTRA)];
    for (off, len) in trozos(muestra, 1024, 4096, 16384) {
        let h = u64::from_le_bytes(blake3::hash(&muestra[off..off + len]).as_bytes()[..8].try_into().unwrap());
        // inserción en la lista de los K menores
        if h >= mins[SKETCH_K - 1] { continue; }
        let mut i = SKETCH_K - 1;
        while i > 0 && mins[i - 1] > h { mins[i] = mins[i - 1]; i -= 1; }
        mins[i] = h;
    }
    mins
}

/// Trocea un flujo sin cargarlo entero: mantiene en memoria como mucho unos pocos `max`.
pub struct TrozosFlujo<R: Read> {
    r: R,
    buf: Vec<u8>,
    ini: usize,
    eof: bool,
    c: Cortador,
}

impl<R: Read> TrozosFlujo<R> {
    pub fn new(r: R, min: usize, avg: usize, max: usize) -> Self {
        TrozosFlujo { r, buf: Vec::with_capacity(max * 3), ini: 0, eof: false, c: Cortador::new(min, avg, max) }
    }

    pub fn siguiente(&mut self) -> std::io::Result<Option<Vec<u8>>> {
        // Hace falta tener al menos `max` disponibles para que el corte sea el mismo que
        // daría el fichero entero en memoria.
        while !self.eof && self.buf.len() - self.ini < self.c.max * 2 {
            let antes = self.buf.len();
            self.buf.resize(antes + 256 * 1024, 0);
            let n = self.r.read(&mut self.buf[antes..])?;
            self.buf.truncate(antes + n);
            if n == 0 {
                self.eof = true;
            }
        }
        if self.ini >= self.buf.len() {
            return Ok(None);
        }
        let len = self.c.corte(&self.buf[self.ini..]);
        let out = self.buf[self.ini..self.ini + len].to_vec();
        self.ini += len;
        if self.ini > self.c.max * 4 {
            self.buf.drain(..self.ini);
            self.ini = 0;
        }
        Ok(Some(out))
    }
}
