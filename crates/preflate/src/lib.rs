/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the Apache License, Version 2.0. See LICENSE.txt in the project root for license information.
 *  This software incorporates material from third parties. See NOTICE.txt for details.
 *--------------------------------------------------------------------------------------------*/

// forbid lints that we already have eliminated from the codebase so they don't show up in the future
#![forbid(unsafe_code)]
#![forbid(trivial_casts)]
#![forbid(trivial_numeric_casts)]
#![forbid(non_ascii_idents)]
#![forbid(redundant_lifetimes)]
#![forbid(single_use_lifetimes)]
#![forbid(macro_use_extern_crate)]
#![forbid(missing_unsafe_on_extern)]
#![forbid(unused_import_braces)]
#![warn(unused_crate_dependencies)]
#![warn(unused_extern_crates)]
#![warn(unused_lifetimes)]
#![warn(unused_macro_rules)]

mod bit_helper;
/// NUESTRO, no suyo: reparte SU coste por canal para poder compararlo con el nuestro.
pub mod contador;
mod cabac_codec;
mod deflate;
mod estimator;
mod hash_algorithm;
mod hash_chain;
mod hash_chain_holder;

mod preflate_error;
mod preflate_input;

mod statistical_codec;
mod stream_processor;
mod token_predictor;
mod tree_predictor;
mod utils;

pub use estimator::preflate_parameter_estimator::TokenPredictorParameters;

/// LOS PARAMETROS QUE preflate LE ADIVINA A UN FLUJO DEFLATE CRUDO.
///
/// AÑADIDO POR NOSOTROS (Vertice Labs, 01-08-2026) sobre el preflate vendorizado. No cambia nada
/// de su logica: solo expone hacia fuera lo que ya calculaba hacia dentro, que es literalmente
/// el motivo por el que se vendorizo («poder tocarlo»).
///
/// Devuelve el codificador que su estimador cree que produjo el flujo —con su algoritmo de hash
/// concreto—, la estrategia (voraz o perezoso con sus umbrales), la politica de que se mete en
/// el diccionario, y la profundidad de cadena que hizo falta. Sirve para CENSAR de que
/// codificadores viene nuestro banco, que es una pregunta que llevabamos contestando por
/// inferencia.
/// NUESTROS, no suyos. El reparto del coste de esta llamada entre sus dos mitades: PARSEAR el
/// DEFLATE —que es exactamente lo mismo que ya hace nuestro inflate, o sea trabajo duplicado— y
/// ESTIMAR de verdad. Sin este reparto no se sabe si merece la pena reutilizar su parseo o si el
/// parseo es calderilla y lo caro es la estimacion.
pub static PF_PARSE_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static PF_EST_NS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn parametros_estimados(deflate: &[u8]) -> Result<TokenPredictorParameters> {
    estimado_y_contenido(deflate).map(|(q, _, _)| q)
}

/// UN SOLO PARSEO: los parametros estimados **y** los tokens, de la MISMA pasada.
///
/// 🔑 POR QUE EXISTE, Y CON QUE MEDIDA. `parametros_estimados` parsea el flujo entero, saca siete
/// campos y TIRA los tokens; quien la llama vuelve a inflar el mismo flujo por su cuenta para
/// tenerlos. Son dos inflates del mismo dato. El reparto de arriba —que se anadio justo para
/// contestar esto— dice cuanto vale cada uno sobre el banco de 2.522 flujos:
///
/// ```text
/// nuestro inflate (vc-core::inflate)                        5,0 s
/// su parseo (parse_deflate_whole), dentro del estimador     2,4 s
/// ```
///
/// O sea que el barato es el SUYO —decodifica Huffman con tabla, el nuestro va bit a bit, que es
/// el metodo de `puff` y esta escrito a proposito para ser demostrable, no rapido— y el que
/// sobra es el nuestro. Esta funcion es lo que permite quitarlo: devuelve lo que ya tenia en la
/// mano en vez de dejarlo caer.
///
/// ⚠️ NO CAMBIA UNA LINEA DE SU LOGICA. Es la misma llamada en el mismo orden; solo deja de
/// tirar el resultado intermedio. Los dos contadores siguen midiendo lo mismo que antes.
///
/// ⚠️ `panic!` incluido: `parse_deflate_whole` revienta si el flujo no esta completo. Quien llama
/// lo envuelve en `catch_unwind` y lo CUENTA (no se silencia).
pub fn estimado_y_contenido(
    deflate: &[u8],
) -> Result<(TokenPredictorParameters, DeflateContents, PlainText)> {
    use std::sync::atomic::Ordering::Relaxed;
    let t = std::time::Instant::now();
    let (contents, plain_text) = deflate::deflate_reader::parse_deflate_whole(deflate)?;
    PF_PARSE_NS.fetch_add(t.elapsed().as_nanos() as u64, Relaxed);
    let t = std::time::Instant::now();
    let r = estimator::preflate_parameter_estimator::estimate_preflate_parameters(&contents, &plain_text);
    PF_EST_NS.fetch_add(t.elapsed().as_nanos() as u64, Relaxed);
    r.map(|q| (q, contents, plain_text))
}

/// NUESTROS, no suyos: los tipos de SU parseo, para poder consumir SUS tokens en vez de volver a
/// inflar el flujo por nuestra cuenta. Solo cambia la visibilidad — el codigo es suyo, intacto.
///
/// ⚠️ Su token NO guarda el SIMBOLO que se emitio (nuestro `sim_long`/`sim_dist`), solo
/// `(long, dist)`. Para la fase 3 da igual —esta medido: el simbolo es derivable salvo el codigo
/// 284, que en este banco no aparece ni una vez— pero para RE-EMITIR el flujo byte a byte no, y
/// por eso el inflate nuestro no se borra: se deja de llamar en el camino caliente.
pub use deflate::deflate_reader::DeflateContents;
pub use deflate::deflate_token::{
    DeflateToken, DeflateTokenBlock, DeflateTokenBlockType, DeflateTokenReference,
};

pub use estimator::depth_estimator::EST_LIMITE;
pub use estimator::add_policy_estimator::DictionaryAddPolicy;
pub use estimator::preflate_parse_config::MatchingType;
pub use hash_algorithm::HashAlgorithm;
/// LOS HASHES REALES, para que vc-fase3 simule el buscador de cada biblioteca en vez de
/// aproximarlo. Se usan LOS SUYOS y no una copia a proposito: un hash es una IDENTIDAD, no un
/// parametro — tiene que coincidir bit a bit o la cadena sale en otro orden. Y la tabla CRC32C
/// son 256 constantes que, copiadas, se desincronizan sin que nadie se entere.
pub use hash_algorithm::{
    Crc32cHash, HashImplementation, LibdeflateHash4, LibdeflateHash4Fast, MiniZHash, ZlibNGHash,
};
pub use preflate_error::ExitCode;
pub use preflate_error::{AddContext, PreflateError, Result, err_exit_code};
pub use preflate_input::{PlainText, PreflateInput};

pub use statistical_codec::{CodecCorrection, CountNonDefaultActions};
pub use contador::{ContadorPreflate, NCAT_PF, RepartoPreflate, es_de_tabla, nombre_canal,
                   reparto_leer, reparto_reiniciar};
pub use stream_processor::{
    PreflateStreamChunkResult, PreflateStreamProcessor, RecreateStreamProcessor,
    preflate_whole_deflate_stream, recreate_whole_deflate_stream,
};

/// Configure the preflate stream processor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreflateConfig {
    /// Maximum number of lookups we will do in the hash chain. This will limit the
    /// amount of CPU time we spend on each chunk.
    pub max_chain_length: u32,

    /// The maximum size of the plain text that will be decompressed to memory before
    /// being recompressed. This limits memory usage and processing time, especially
    /// for files that are extremely compressed (eg zip bombs)
    pub plain_text_limit: usize,

    /// Decompress the data and verify that it can be recompressed to exactly the same bytes.
    /// This will slow down processing is isn't necessary if you are already doing end-to-end
    /// verification of the data.
    pub verify_compression: bool,
}

impl Default for PreflateConfig {
    fn default() -> Self {
        Self {
            max_chain_length: 4096,
            plain_text_limit: 128 * 1024 * 1024,
            verify_compression: true,
        }
    }
}

#[cfg(test)]
static INIT: std::sync::Once = std::sync::Once::new();

/// Initialize the logger for tests. This is a no-op if the logger is already initialized.
#[cfg(test)]
pub fn init_logging() {
    INIT.call_once(|| {
        let _ = env_logger::builder().is_test(true).try_init();
    });
}
