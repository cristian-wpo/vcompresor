//! EL REPARTO DE SU PROPIO COSTE, contado con NUESTRO mismo convenio.
//!
//! 🔑 POR QUE EXISTE. Publicabamos «preflate 1,700% contra nosotros 2,218%» sin decir que las dos
//! cifras no miden lo mismo. La suya es `corr.len()` de `preflate_whole_deflate_stream`, o sea su
//! salida COMPLETA: parametros, tokens, estructura de bloques Y las correcciones de las TABLAS DE
//! HUFFMAN — siete de sus diecisiete canales (`TreeCodeBitLengthCorrection`, `LDTypeCorrection`,
//! `RepeatCountCorrection`, `LDBitLengthCorrection`, `TreeCodeCountCorrection`,
//! `LiteralCountCorrection`, `DistanceCountCorrection`). La nuestra cuenta SOLO tokens y no
//! modela las tablas en absoluto. La comparacion nos favorecia y no estaba dicho.
//!
//! Esto lo reparte. No toca su logica: `impl<A, B> PredictionEncoder for (A, B)` —el derivador que
//! ya tenian escrito para verificar dos codificadores— deja colgar un contador al lado del
//! codificador de verdad. Ve exactamente las mismas llamadas, en el mismo orden.
//!
//! ⚠️ QUE MIDE Y QUE NO. No cuenta los bits que produce su aritmetico —eso exigiria reimplementar
//! su modelo de probabilidades—: cuenta las mismas magnitudes que cobramos nosotros, con la misma
//! regla de rachas de `encode_correction` (cabac_codec.rs:133) y el mismo `value - 1` de
//! `write_exp_encoded`. Por eso el reparto ES comparable con nuestro `coste_pf`, que es de lo que
//! se trata. Y el total modelado se contrasta contra sus bytes REALES: si no se parecen, el
//! reparto no vale y hay que decirlo en vez de publicarlo.

use crate::statistical_codec::{CodecCorrection, PredictionEncoder};
use std::cell::RefCell;

/// Cuantos canales tiene su enum. Se lee de el, no se teclea: si añaden uno, esto crece solo.
pub const NCAT_PF: usize = CodecCorrection::MAX as usize;
const NBL: usize = 40;

#[derive(Clone, Copy)]
pub struct RepartoPreflate {
    /// Por canal, histograma de la longitud en bits de las LONGITUDES DE RACHA.
    pub bl_racha: [[u64; NBL]; NCAT_PF],
    /// Por canal, histograma de la longitud en bits de los VALORES de correccion.
    pub bl_valor: [[u64; NBL]; NCAT_PF],
}

impl Default for RepartoPreflate {
    fn default() -> Self {
        RepartoPreflate { bl_racha: [[0; NBL]; NCAT_PF], bl_valor: [[0; NBL]; NCAT_PF] }
    }
}

thread_local! {
    static REPARTO: RefCell<RepartoPreflate> = RefCell::new(RepartoPreflate::default());
}

/// Pone el contador a cero. Se llama antes de cada corrida, no antes de cada flujo: el reparto
/// que interesa es el del banco entero.
pub fn reparto_reiniciar() {
    REPARTO.with(|r| *r.borrow_mut() = RepartoPreflate::default());
}

/// Lo acumulado hasta ahora.
pub fn reparto_leer() -> RepartoPreflate {
    REPARTO.with(|r| *r.borrow())
}

/// El nombre de cada canal, para imprimirlo sin teclear una segunda lista que se desincronice.
pub fn nombre_canal(i: usize) -> &'static str {
    use CodecCorrection::*;
    const N: [(CodecCorrection, &str); 17] = [
        (TokenCount, "TokenCount"),
        (BlockTypeCorrection, "BlockType"),
        (LenCorrection, "longitud"),
        (DistOnlyCorrection, "distancia"),
        (DistAfterLenCorrection, "dist-tras-long"),
        (TreeCodeBitLengthCorrection, "TABLA: bits del codigo"),
        (LDTypeCorrection, "TABLA: tipo LD"),
        (RepeatCountCorrection, "TABLA: repeticiones"),
        (LDBitLengthCorrection, "TABLA: bits LD"),
        (TreeCodeCountCorrection, "TABLA: cuantos codigos"),
        (LiteralCountCorrection, "TABLA: cuantos literales"),
        (DistanceCountCorrection, "TABLA: cuantas distancias"),
        (UncompressBlockLenCorrection, "bloque almacenado"),
        (Last, "ultimo bloque"),
        (EndOfChunk, "fin de tramo"),
        (LiteralPredictionWrong, "lit?"),
        (ReferencePredictionWrong, "copia?"),
    ];
    for (c, n) in N.iter() { if *c as usize == i { return *n } }
    "?"
}

/// ¿Es este canal de las TABLAS DE HUFFMAN? Es la pregunta que motivo todo esto.
pub fn es_de_tabla(i: usize) -> bool {
    use CodecCorrection::*;
    i == TreeCodeBitLengthCorrection as usize
        || i == LDTypeCorrection as usize
        || i == RepeatCountCorrection as usize
        || i == LDBitLengthCorrection as usize
        || i == TreeCodeCountCorrection as usize
        || i == LiteralCountCorrection as usize
        || i == DistanceCountCorrection as usize
}

#[inline]
fn bitlen(v: u64) -> usize { ((64 - v.leading_zeros()) as usize).min(NBL - 1) }

/// Un codificador que NO ESCRIBE NADA: solo cuenta. Cuelga del derivador `(A, B)` al lado del
/// codificador de verdad, asi que ve las mismas llamadas en el mismo orden y no puede alterar ni
/// un byte de la salida.
#[derive(Default)]
pub struct ContadorPreflate {
    racha: [u64; NCAT_PF],
}

impl PredictionEncoder for ContadorPreflate {
    fn encode_verify_state(&mut self, _mensaje: &'static str, _suma: u64) {}

    fn encode_correction(&mut self, canal: CodecCorrection, valor: u32) {
        let i = canal as usize;
        if i >= NCAT_PF { return }
        // La MISMA regla que `PredictionEncoderCabac::encode_correction`: un cero no emite nada,
        // alarga la racha abierta de su canal.
        if valor == 0 { self.racha[i] += 1; return }
        REPARTO.with(|r| {
            let mut r = r.borrow_mut();
            if self.racha[i] > 0 { r.bl_racha[i][bitlen(self.racha[i] - 1)] += 1 }
            r.bl_valor[i][bitlen((valor - 1) as u64)] += 1;
        });
        self.racha[i] = 0;
    }

    fn finish(&mut self) {
        // `finish()` vuelca las rachas abiertas, igual que el suyo.
        REPARTO.with(|r| {
            let mut r = r.borrow_mut();
            for i in 0..NCAT_PF {
                if self.racha[i] > 0 {
                    r.bl_racha[i][bitlen(self.racha[i] - 1)] += 1;
                    self.racha[i] = 0;
                }
            }
        });
    }
}
