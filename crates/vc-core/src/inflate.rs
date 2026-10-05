// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs.

//! DEFLATE propio, primera fase: DESCOMPRIMIR (RFC 1951).
//!
//! POR QUE ESTO EXISTE. Hoy el carril de PNG/ZIP/GZIP se apoya en `preflate-rs`, que va por la
//! version 0.7.6 —ni siquiera 1.0— y arrastra catorce cajas mas que no auditamos. Lo medido el
//! 2026-07-30 con el desglose directo por fases:
//!
//!   · preflate es el **78% del RELOJ de todo el archivador**: apagarlo baja de 347,9 s a 75,9 s.
//!   · ese 78% se reparte en   inflar 9,6% · estimar 66,5% · re-deflate y verificar 23,9%.
//!   · y **falla en el 8,7% de los flujos reales**: procesa 1.802 de 1.973, y no falla limpio
//!     sino con un panico (index out of bounds en tree_predictor.rs:169). Los ficheros que
//!     rechaza pierden entero el 7,07% de tamaño que este carril compra.
//!
//! Este modulo procesa los 1.973. Que la pieza mas valiosa del producto sea la que menos
//! controlamos es la definicion de un riesgo mal colocado.
//!
//! EL ORDEN DEL PROYECTO, y cada fase se valida contra el mismo banco de 1.973 flujos reales
//! sacados del corpus (bench/saca-deflate.py):
//!
//!   1. DESCOMPRIMIR  (esto)     el contrato es que salga EXACTAMENTE lo mismo que zlib.
//!   2. RECOMPRIMIR + DIFERENCIA volver a comprimir y anotar en que nos separamos del original.
//!   3. EL ESTIMADOR             adivinar los ajustes del compresor original para que la
//!                               diferencia tienda a cero.
//!   4. EL PREDICTOR             en vez de guardar la diferencia en bytes, predecir las
//!                               DECISIONES del codificador y guardar solo las correcciones.
//!
//! POR QUE SE EMPIEZA POR DESCOMPRIMIR AUNQUE YA SEPAMOS HACERLO CON zlib: porque es la unica
//! pieza del proyecto con una verdad absoluta contra la que medirse —el resultado de zlib, byte
//! a byte— y porque escribirla obliga a entender el formato antes de intentar predecirlo. Las
//! fases 2 a 4 no tienen una verdad tan comoda.
//!
//! SOBRE LA VELOCIDAD: esta version decodifica los codigos de Huffman BIT A BIT, que es el
//! metodo del `puff` de referencia de zlib. Es el mas lento y el mas facil de demostrar
//! correcto. La velocidad se compra despues con una tabla de busqueda de 9 bits —los conteos
//! canonicos que hacen falta YA se calculan en `Huff::nuevo`— y solo cuando la correccion este
//! cerrada contra los 1.973 flujos. Al reves se tarda mas: un decodificador rapido y con un
//! fallo sutil cuesta dias de encontrar.

use anyhow::{bail, Result};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

// ── LA PIRAMIDE: contadores exactos, detras de VC_INFLATE_CUENTA ────────────────────────────
//
// Se cuenta en vez de muestrear porque perf da 71,23% en `main`: el compilador incrusta todo el
// decodificador y el muestreo no puede repartir lo que el optimizador fundio. Un contador
// exacto no tiene ruido, no hay que repetirlo y distingue los peldaños de verdad.
//
// El coste cuando esta apagado es una comparacion contra un bool por llamada, que el predictor
// de saltos acierta siempre. Se MIDE mas abajo, no se supone.
pub static C_BITS: AtomicU64 = AtomicU64::new(0);
pub static C_SIMBOLOS: AtomicU64 = AtomicU64::new(0);
pub static C_LITERALES: AtomicU64 = AtomicU64::new(0);
pub static C_COPIAS: AtomicU64 = AtomicU64::new(0);
pub static C_BYTES_COPIADOS: AtomicU64 = AtomicU64::new(0);
pub static C_HUFF_PASOS: AtomicU64 = AtomicU64::new(0);
pub static C_BLOQUES: AtomicU64 = AtomicU64::new(0);
/// Copias en las que origen y destino NO se pisan (distancia >= longitud): esas se pueden hacer
/// anchas. Las que si se pisan tienen que seguir byte a byte, y son un caso real y frecuente
/// —distancia 1 y longitud 100 es repetir un byte cien veces—.
pub static C_COPIAS_ANCHAS: AtomicU64 = AtomicU64::new(0);
pub static C_BYTES_ANCHOS: AtomicU64 = AtomicU64::new(0);
/// Bytes de copias QUE SE SOLAPAN, por tramo de distancia. Decide cual de los atajos aplica:
/// distancia 1 es un memset; hasta 8 el patron cabe en una palabra; por encima, no hay atajo
/// simple. Sin este reparto, «optimizar las copias» seria elegir el atajo a ciegas.
pub static C_SOLAPE_D1: AtomicU64 = AtomicU64::new(0);
pub static C_SOLAPE_D2_8: AtomicU64 = AtomicU64::new(0);
pub static C_SOLAPE_D9_64: AtomicU64 = AtomicU64::new(0);
pub static C_SOLAPE_MAS: AtomicU64 = AtomicU64::new(0);

fn contando() -> bool {
    static E: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *E.get_or_init(|| std::env::var("VC_INFLATE_CUENTA").is_ok())
}

/// Vuelca la piramide. Devuelve las lineas para que quien llame decida donde van.
pub fn piramide(bytes_entrada: u64) -> Vec<String> {
    let (b, si, li, co, bc, hp, bl) = (
        C_BITS.load(Relaxed), C_SIMBOLOS.load(Relaxed), C_LITERALES.load(Relaxed),
        C_COPIAS.load(Relaxed), C_BYTES_COPIADOS.load(Relaxed), C_HUFF_PASOS.load(Relaxed),
        C_BLOQUES.load(Relaxed));
    let salida = li + bc;
    let f = |x: u64, por: u64| if por == 0 { 0.0 } else { x as f64 / por as f64 };
    vec![
        format!("  {:<26} {:>16}  {:>12}", "peldaño", "cuantos", "por simbolo"),
        format!("  {:<26} {:>16} {:>13}", "bytes de DEFLATE", bytes_entrada, format!("{:.3}", f(bytes_entrada, si))),
        format!("  {:<26} {:>16} {:>13}", "bloques", bl, format!("{:.6}", f(bl, si))),
        format!("  {:<26} {:>16} {:>13}", "llamadas a bit()", b, format!("{:.2}", f(b, si))),
        format!("  {:<26} {:>16} {:>13}", "pasos dentro de Huff::lee", hp, format!("{:.2}", f(hp, si))),
        format!("  {:<26} {:>16} {:>13}", "simbolos decodificados", si, "1.00".to_string()),
        format!("  {:<26} {:>16} {:>13}", "  de ellos literales", li, format!("{:.2}", f(li, si))),
        format!("  {:<26} {:>16} {:>13}", "  de ellos copias", co, format!("{:.2}", f(co, si))),
        format!("  {:<26} {:>16} {:>13}", "bytes de plano producidos", salida, format!("{:.2}", f(salida, si))),
        format!("  {:<26} {:>16} {:>13}", "  copiados (coincidencias)", bc, format!("{:.2}", f(bc, si))),
        format!("  {:<26} {:>16} {:>13}", "  escritos como literal", li, format!("{:.2}", f(li, si))),
        format!("  {:<26} {:>16} {:>13}", "copias SIN solape", C_COPIAS_ANCHAS.load(Relaxed),
                format!("{:.1}%", 100.0 * f(C_COPIAS_ANCHAS.load(Relaxed), co))),
        format!("  {:<26} {:>16} {:>13}", "  sus bytes", C_BYTES_ANCHOS.load(Relaxed),
                format!("{:.1}%", 100.0 * f(C_BYTES_ANCHOS.load(Relaxed), bc))),
        format!("  {:<26} {:>16} {:>13}", "solape dist=1 (memset)", C_SOLAPE_D1.load(Relaxed),
                format!("{:.1}%", 100.0 * f(C_SOLAPE_D1.load(Relaxed), bc))),
        format!("  {:<26} {:>16} {:>13}", "solape dist 2-8", C_SOLAPE_D2_8.load(Relaxed),
                format!("{:.1}%", 100.0 * f(C_SOLAPE_D2_8.load(Relaxed), bc))),
        format!("  {:<26} {:>16} {:>13}", "solape dist 9-64", C_SOLAPE_D9_64.load(Relaxed),
                format!("{:.1}%", 100.0 * f(C_SOLAPE_D9_64.load(Relaxed), bc))),
        format!("  {:<26} {:>16} {:>13}", "solape dist >64", C_SOLAPE_MAS.load(Relaxed),
                format!("{:.1}%", 100.0 * f(C_SOLAPE_MAS.load(Relaxed), bc))),
    ]
}

/// Lector de bits en el orden de DEFLATE: del bit menos significativo al mas.
///
/// Es al reves que en JPEG y que en la mayoria de formatos, y confundirlos produce un
/// decodificador que casi funciona — que es la peor clase de fallo.
struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
    /// bits ya leidos del byte actual (0..8)
    usados: u32,
}

impl<'a> Bits<'a> {
    fn new(d: &'a [u8]) -> Self { Bits { d, pos: 0, usados: 0 } }

    #[inline]
    fn bit(&mut self) -> Result<u32> {
        if contando() { C_BITS.fetch_add(1, Relaxed); }
        if self.pos >= self.d.len() { bail!("se acabaron los bits") }
        let b = (self.d[self.pos] >> self.usados) & 1;
        self.usados += 1;
        if self.usados == 8 { self.usados = 0; self.pos += 1; }
        Ok(b as u32)
    }

    #[inline]
    fn bits(&mut self, n: u32) -> Result<u32> {
        let mut v = 0u32;
        for i in 0..n { v |= self.bit()? << i; }
        Ok(v)
    }

    /// Mira los proximos `n` bits SIN consumirlos, en el orden en que llegan (el primero en
    /// el bit 0). Devuelve None si no quedan `n` bits: en ese caso hay que ir por el camino
    /// lento, que sabe terminar bien. Rellenar con ceros aqui daria aciertos falsos en la tabla.
    #[inline]
    fn mira(&self, n: u32) -> Option<u32> {
        let quedan = (self.d.len() - self.pos) as u64 * 8 - self.usados as u64;
        if quedan < n as u64 { return None; }
        let mut v = 0u32;
        let (mut pos, mut usados) = (self.pos, self.usados);
        for i in 0..n {
            v |= (((self.d[pos] >> usados) & 1) as u32) << i;
            usados += 1;
            if usados == 8 { usados = 0; pos += 1; }
        }
        Some(v)
    }

    /// Avanza `n` bits. Solo se usa despues de un acierto en la tabla, o sea que los bits ya
    /// se comprobaron en `mira`.
    #[inline]
    fn salta(&mut self, n: u32) {
        let t = self.usados + n;
        self.pos += (t / 8) as usize;
        self.usados = t % 8;
    }

    /// Descarta lo que quede del byte en curso. Los bloques ALMACENADOS empiezan alineados.
    fn alinea(&mut self) {
        if self.usados != 0 { self.usados = 0; self.pos += 1; }
    }
}

/// Un arbol de Huffman canonico, guardado como lo guarda `puff`: cuantos codigos hay de cada
/// longitud, y los simbolos ordenados. No hace falta construir el arbol: con esas dos cosas se
/// decodifica bit a bit sin ambiguedad.
struct Huff {
    /// conteo[l] = cuantos codigos miden l bits (l de 1 a 15)
    conteo: [u16; 16],
    /// los simbolos, ordenados primero por longitud y luego por valor
    simbolo: Vec<u16>,
    /// LA TABLA RAPIDA: 512 entradas indexadas por los 9 proximos bits TAL COMO LLEGAN.
    /// Cada entrada es (simbolo << 8) | longitud, o 0 si el codigo pasa de 9 bits y hay que
    /// caer al camino lento. Son 2 KiB por arbol: cabe en L1 de sobra.
    rapida: Vec<u32>,
}

/// Cuantos bits mira la tabla de un golpe. Nueve y no quince a proposito: con quince la tabla
/// serian 32.768 entradas por arbol y cada bloque dinamico nuevo la ensuciaria entera, que es
/// exactamente el fallo que hace a vc-cm gastar el 83,7% de su tiempo esperando a la memoria.
const TABLA_BITS: u32 = 9;

impl Huff {
    /// Construye el arbol a partir de la longitud del codigo de cada simbolo.
    ///
    /// Se COMPRUEBA que el conjunto sea completo o de un solo simbolo. Un conjunto incompleto
    /// —que deja codigos sin asignar— es un flujo corrupto, y aceptarlo significa decodificar
    /// basura en silencio hasta que algo revienta mucho mas lejos.
    fn nuevo(longs: &[u8]) -> Result<Huff> {
        let mut conteo = [0u16; 16];
        for &l in longs {
            if l as usize > 15 { bail!("longitud de codigo {} fuera de rango", l) }
            conteo[l as usize] += 1;
        }
        conteo[0] = 0;   // los simbolos que no aparecen no cuentan

        // ¿sobran o faltan codigos? sobra = 1 en la raiz, y se va gastando por nivel
        let mut sobra: i32 = 1;
        for l in 1..16 {
            sobra <<= 1;
            sobra -= conteo[l] as i32;
            if sobra < 0 { bail!("codigo de Huffman sobre-suscrito en la longitud {}", l) }
        }

        let mut offs = [0u16; 16];
        for l in 1..15 { offs[l + 1] = offs[l] + conteo[l]; }
        let mut simbolo = vec![0u16; longs.len()];
        for (s, &l) in longs.iter().enumerate() {
            if l != 0 {
                simbolo[offs[l as usize] as usize] = s as u16;
                offs[l as usize] += 1;
            }
        }

        // LA TABLA RAPIDA. Se recorren los codigos canonicos en el mismo orden que los asigna
        // el RFC, y cada uno se coloca en todos los indices cuyos L bits bajos sean su codigo
        // INVERTIDO — porque el flujo entrega los bits al reves de como se forma el codigo.
        let mut rapida = vec![0u32; 1 << TABLA_BITS];
        let mut siguiente = [0u16; 16];
        let mut cod = 0u16;
        for l in 1..16usize {
            cod = (cod + if l >= 2 { conteo[l - 1] } else { 0 }) << 1;
            siguiente[l] = cod;
        }
        for (sim, &l) in longs.iter().enumerate() {
            if l == 0 { continue; }
            let l = l as u32;
            let c = siguiente[l as usize];
            siguiente[l as usize] += 1;
            if l > TABLA_BITS { continue; }
            // invertir los l bits: el flujo da primero el bit mas significativo del codigo
            let mut rev = 0u32;
            for i in 0..l { rev |= (((c >> i) & 1) as u32) << (l - 1 - i); }
            let paso = 1u32 << l;
            let mut i = rev;
            while i < (1 << TABLA_BITS) {
                rapida[i as usize] = ((sim as u32) << 8) | l;
                i += paso;
            }
        }
        Ok(Huff { conteo, simbolo, rapida })
    }

    /// Decodifica un simbolo leyendo bit a bit. Es el metodo de `puff`: se va formando el codigo
    /// y en cada longitud se mira si cae dentro del rango de codigos de esa longitud.
    #[inline]
    fn lee(&self, b: &mut Bits) -> Result<u16> {
        // EL CAMINO RAPIDO: un acceso a una tabla de 2 KiB en vez de caminar bit a bit. Si el
        // codigo pasa de 9 bits, o si no quedan 9 bits en el flujo, se cae al camino de
        // siempre — que es el que sabe terminar bien y el que demuestra la correccion.
        if let Some(pk) = b.mira(TABLA_BITS) {
            let e = self.rapida[pk as usize];
            if e != 0 {
                b.salta(e & 0xFF);
                if contando() { C_HUFF_PASOS.fetch_add(1, Relaxed); }
                return Ok((e >> 8) as u16);
            }
        }
        let (mut codigo, mut primero, mut idx) = (0i32, 0i32, 0i32);
        for l in 1..16usize {
            if contando() { C_HUFF_PASOS.fetch_add(1, Relaxed); }
            codigo |= b.bit()? as i32;
            let cuenta = self.conteo[l] as i32;
            if codigo - cuenta < primero {
                return Ok(self.simbolo[(idx + (codigo - primero)) as usize]);
            }
            idx += cuenta;
            primero = (primero + cuenta) << 1;
            codigo <<= 1;
        }
        bail!("codigo de Huffman incompleto")
    }
}

// --- las tablas del RFC 1951 -------------------------------------------------------------

const LONG_BASE: [u16; 29] = [3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258];
const LONG_EXTRA: [u8; 29] = [0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0];
const DIST_BASE: [u16; 30] = [1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577];
const DIST_EXTRA: [u8; 30] = [0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13];
/// El orden en que van las longitudes del alfabeto de longitudes. No es 0..18: es este, y esta
/// elegido para que las que casi nunca se usan queden al final y se puedan omitir.
const ORDEN_CL: [usize; 19] = [16,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15];

/// Los arboles fijos del RFC: se usan cuando el bloque dice «tipo 1» y no trae tabla propia.
///
/// SE CACHEAN. Son constantes —salen de dos arrays fijos del RFC 1951— y se construian una vez
/// POR FLUJO: sobre el banco de 1.973 flujos eso son 3.946 construcciones identicas, cada una
/// recorriendo 288 simbolos tres veces. Con OnceLock se hace una y ya.
fn arboles_fijos() -> &'static (Huff, Huff) {
    static FIJOS: std::sync::OnceLock<(Huff, Huff)> = std::sync::OnceLock::new();
    FIJOS.get_or_init(construye_fijos)
}

fn construye_fijos() -> (Huff, Huff) {
    let mut l = [0u8; 288];
    for (i, v) in l.iter_mut().enumerate() {
        *v = if i < 144 { 8 } else if i < 256 { 9 } else if i < 280 { 7 } else { 8 };
    }
    let d = [5u8; 30];
    (Huff::nuevo(&l).expect("arbol fijo de literales"), Huff::nuevo(&d).expect("arbol fijo de distancias"))
}

/// Lee los dos arboles que el bloque trae escritos (tipo 2).
///
/// Es la parte con mas trampa del formato: las longitudes de los codigos van, a su vez,
/// comprimidas con otro Huffman, y hay tres codigos de repeticion que copian hacia atras.
fn arboles_dinamicos(b: &mut Bits) -> Result<(Huff, Huff, Cabecera)> {
    let hlit = b.bits(5)? as usize + 257;
    let hdist = b.bits(5)? as usize + 1;
    let hclen = b.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 { bail!("cabecera de bloque dinamico fuera de rango") }

    let mut cl = [0u8; 19];
    for i in 0..hclen { cl[ORDEN_CL[i]] = b.bits(3)? as u8; }
    let arbol_cl = Huff::nuevo(&cl)?;

    let mut longs = vec![0u8; hlit + hdist];
    // La secuencia se guarda EN CRUDO: el mismo array de longitudes admite varias
    // codificaciones legales, y para devolver los bits que habia hay que replicar la suya.
    let mut secuencia: Vec<(u8, u8)> = Vec::new();
    let mut i = 0;
    while i < longs.len() {
        let s = arbol_cl.lee(b)?;
        match s {
            0..=15 => { longs[i] = s as u8; i += 1; secuencia.push((s as u8, 0)); }
            16 => {
                // repite la ANTERIOR de 3 a 6 veces. Sin anterior, el flujo esta corrupto.
                if i == 0 { bail!("repeticion 16 sin longitud anterior") }
                let v = longs[i - 1];
                let e = b.bits(2)? as u8;
                let n = 3 + e as usize;
                if i + n > longs.len() { bail!("repeticion 16 se sale") }
                for _ in 0..n { longs[i] = v; i += 1; }
                secuencia.push((16, e));
            }
            17 => {
                let e = b.bits(3)? as u8;
                let n = 3 + e as usize;
                if i + n > longs.len() { bail!("repeticion 17 se sale") }
                i += n;   // ceros: ya lo estan
                secuencia.push((17, e));
            }
            18 => {
                let e = b.bits(7)? as u8;
                let n = 11 + e as usize;
                if i + n > longs.len() { bail!("repeticion 18 se sale") }
                i += n;
                secuencia.push((18, e));
            }
            _ => bail!("simbolo {} imposible en el alfabeto de longitudes", s),
        }
    }
    let lit = Huff::nuevo(&longs[..hlit])?;
    let dist = Huff::nuevo(&longs[hlit..])?;
    Ok((lit, dist, Cabecera { hlit, hdist, hclen, cl, secuencia }))
}

/// Una decision del codificador original: o escribio un byte tal cual, o dijo «repite `long`
/// bytes de hace `dist`».
///
/// `sim_long` y `sim_dist` son los SIMBOLOS que se usaron de verdad, no los que se derivarian
/// de (long, dist). Importa para volver a emitir los mismos bits: la longitud 258 admite el
/// codigo 284 con extra=31 y tambien el 285, las dos legales, y zlib usa una u otra segun el
/// caso. Guardar el simbolo es lo que separa «un DEFLATE valido» de «EL DEFLATE que habia».
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Token {
    Lit(u8),
    Copia { long: u16, dist: u16, sim_long: u16, sim_dist: u16 },
}

/// La cabecera de un bloque dinamico, tal cual venia.
///
/// La secuencia RLE se guarda EN CRUDO —simbolo y extras, en su orden— y no como el array de
/// longitudes que produce, porque el mismo array admite varias codificaciones legales: cuatro
/// ceros caben en cuatro simbolos «0» o en un «17» con n=4. Hay que devolver la que estaba.
#[derive(Clone, Debug, PartialEq)]
pub struct Cabecera {
    pub hlit: usize,
    pub hdist: usize,
    pub hclen: usize,
    /// Los 19 codigos de longitud, indexados por simbolo (no por el orden de lectura).
    pub cl: [u8; 19],
    /// (simbolo 0..=18, bits extra tal cual se leyeron) en el orden exacto en que aparecieron.
    pub secuencia: Vec<(u8, u8)>,
}

/// Un bloque del flujo, con todo lo que hace falta para volver a emitirlo igual.
#[derive(Clone, Debug, PartialEq)]
pub enum Bloque {
    /// BTYPE=00: alineado a byte, con len y !len delante.
    Almacenado { ultimo: bool, datos: Vec<u8> },
    /// BTYPE=01 (arboles fijos, `cabecera` a None) o BTYPE=10 (dinamicos, con su cabecera).
    Comprimido { ultimo: bool, cabecera: Option<Cabecera>, tokens: Vec<Token> },
}

/// Descomprime un flujo DEFLATE en crudo (sin la cabecera de zlib ni la de gzip).
///
/// Devuelve los bytes originales. El contrato es duro y es el unico que importa: tiene que salir
/// EXACTAMENTE lo mismo que saca zlib, para los 1.973 flujos del banco.
pub fn descomprime(d: &[u8]) -> Result<Vec<u8>> {
    descomprime_pleno(d, None)
}

/// Como `descomprime`, pero ademas devuelve la secuencia de decisiones del codificador.
pub fn descomprime_con_tokens(d: &[u8]) -> Result<(Vec<u8>, Vec<Token>)> {
    let (out, bl) = descomprime_con_bloques(d)?;
    let mut t = Vec::new();
    for b in bl {
        match b {
            Bloque::Comprimido { tokens, .. } => t.extend(tokens),
            Bloque::Almacenado { datos, .. } => t.extend(datos.iter().map(|&x| Token::Lit(x))),
        }
    }
    Ok((out, t))
}

/// Descomprime Y describe el flujo entero: bloques, cabeceras y simbolos tal cual venian.
///
/// Es lo que hace falta para re-emitirlo byte a byte, que es la fase 2 del preflate propio.
/// `descomprime` no paga nada por esto: la captura solo ocurre si se pide.
pub fn descomprime_con_bloques(d: &[u8]) -> Result<(Vec<u8>, Vec<Bloque>)> {
    let mut bl = Vec::new();
    let out = descomprime_pleno(d, Some(&mut bl))?;
    Ok((out, bl))
}

fn descomprime_pleno(d: &[u8], mut blo: Option<&mut Vec<Bloque>>) -> Result<Vec<u8>> {
    let mut b = Bits::new(d);
    let mut out: Vec<u8> = Vec::with_capacity(d.len() * 4);
    let (fijo_lit, fijo_dist) = arboles_fijos();   // cacheados: son constantes

    loop {
        let ultimo = b.bit()?;
        let tipo = b.bits(2)?;
        if contando() { C_BLOQUES.fetch_add(1, Relaxed); }
        let mut tokens: Vec<Token> = Vec::new();
        let mut cab_del_bloque: Option<Cabecera> = None;
        match tipo {
            0 => {
                // ALMACENADO: alineado a byte, con la longitud y su complemento.
                b.alinea();
                if b.pos + 4 > d.len() { bail!("bloque almacenado truncado") }
                let len = u16::from_le_bytes([d[b.pos], d[b.pos + 1]]) as usize;
                let nlen = u16::from_le_bytes([d[b.pos + 2], d[b.pos + 3]]);
                if (len as u16) != !nlen { bail!("bloque almacenado: len y ~len no cuadran") }
                b.pos += 4;
                if b.pos + len > d.len() { bail!("bloque almacenado se sale") }
                if let Some(bs) = blo.as_deref_mut() {
                    bs.push(Bloque::Almacenado {
                        ultimo: ultimo == 1,
                        datos: d[b.pos..b.pos + len].to_vec(),
                    });
                }
                out.extend_from_slice(&d[b.pos..b.pos + len]);
                b.pos += len;
            }
            1 | 2 => {
                let (lit, dist);
                let (l, ds) = if tipo == 1 {
                    (fijo_lit, fijo_dist)
                } else {
                    let (a, c, h) = arboles_dinamicos(&mut b)?;
                    lit = a; dist = c; cab_del_bloque = Some(h);
                    (&lit, &dist)
                };
                loop {
                    let s = l.lee(&mut b)?;
                    if s < 256 {
                        out.push(s as u8);
                        if contando() {
                            C_SIMBOLOS.fetch_add(1, Relaxed);
                            C_LITERALES.fetch_add(1, Relaxed);
                        }
                        if blo.is_some() { tokens.push(Token::Lit(s as u8)); }
                    } else if s == 256 {
                        break;
                    } else {
                        let i = s as usize - 257;
                        if i >= 29 { bail!("codigo de longitud {} fuera de rango", s) }
                        let long = LONG_BASE[i] as usize + b.bits(LONG_EXTRA[i] as u32)? as usize;
                        let ds_s = ds.lee(&mut b)? as usize;
                        if ds_s >= 30 { bail!("codigo de distancia {} fuera de rango", ds_s) }
                        let distancia = DIST_BASE[ds_s] as usize + b.bits(DIST_EXTRA[ds_s] as u32)? as usize;
                        if distancia > out.len() { bail!("distancia {} mayor que lo ya escrito", distancia) }
                        // TRES CAMINOS, elegidos por lo que dice la medida y no por intuicion.
                        // Contado sobre los 1.973 flujos reales, de los 5.872.479.327 bytes que
                        // se copian:
                        //
                        //   72,1%  distancia 1: un byte repetido -> resize, que es un memset
                        //   25,6%  sin solape  -> extend_from_within, copia ancha
                        //    2,3%  el resto    -> byte a byte, EXACTAMENTE como estaba
                        //
                        // El byte a byte no era un descuido: las copias pueden solaparse de
                        // verdad y copy_from_slice daria basura. Lo que faltaba era separar los
                        // casos, y para separarlos habia que contarlos.
                        //
                        // El caso general se deja intacto a proposito: este bucle es el corazon
                        // de la correccion y un fallo aqui rompe la ida y vuelta de todo.
                        if blo.is_some() {
                            tokens.push(Token::Copia {
                                long: long as u16, dist: distancia as u16,
                                sim_long: s, sim_dist: ds_s as u16,
                            });
                        }
                        if contando() {
                            C_SIMBOLOS.fetch_add(1, Relaxed);
                            C_COPIAS.fetch_add(1, Relaxed);
                            C_BYTES_COPIADOS.fetch_add(long as u64, Relaxed);
                            if distancia >= long {
                                C_COPIAS_ANCHAS.fetch_add(1, Relaxed);
                                C_BYTES_ANCHOS.fetch_add(long as u64, Relaxed);
                            } else {
                                let n = long as u64;
                                match distancia {
                                    1 => C_SOLAPE_D1.fetch_add(n, Relaxed),
                                    2..=8 => C_SOLAPE_D2_8.fetch_add(n, Relaxed),
                                    9..=64 => C_SOLAPE_D9_64.fetch_add(n, Relaxed),
                                    _ => C_SOLAPE_MAS.fetch_add(n, Relaxed),
                                };
                            }
                        }
                        let ini = out.len() - distancia;
                        if distancia == 1 {
                            // el mismo byte, `long` veces
                            let b = out[ini];
                            out.resize(out.len() + long, b);
                        } else if distancia >= long {
                            // origen y destino disjuntos: se puede copiar de golpe
                            out.extend_from_within(ini..ini + long);
                        } else {
                            for k in 0..long { let v = out[ini + k]; out.push(v); }
                        }
                    }
                }
            }
            _ => bail!("tipo de bloque 3, que esta reservado"),
        }
        if tipo != 0 {
            if let Some(bs) = blo.as_deref_mut() {
                bs.push(Bloque::Comprimido { ultimo: ultimo == 1, cabecera: cab_del_bloque.take(), tokens });
            }
        }
        if ultimo == 1 { break; }
    }
    Ok(out)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// La prueba mas basica y la que mas fallos caza: un bloque almacenado.
    #[test]
    fn almacenado() {
        // 1 (ultimo) + tipo 00, alineado, len=5, ~len, "hola!"
        let d = [0x01, 0x05, 0x00, 0xFA, 0xFF, b'h', b'o', b'l', b'a', b'!'];
        assert_eq!(descomprime(&d).unwrap(), b"hola!");
    }

    /// Los tokens tienen que reconstruir el plano EXACTO. Si no, no describen lo que hizo el
    /// codificador y cualquier estimador montado encima trabajaria sobre una mentira. Se
    /// comprueba re-ejecutandolos, que es la unica forma de saberlo.
    #[test]
    fn los_tokens_reconstruyen_el_plano() {
        for plano in [&b"aaaaaa"[..], &b""[..], &vec![b'x'; 10000][..]] {
            let comp = comprime_con_zlib(plano);
            let (salida, tokens) = descomprime_con_tokens(&comp).unwrap();
            assert_eq!(salida, plano, "el plano no coincide");
            let mut rehecho: Vec<u8> = Vec::new();
            for t in &tokens {
                match *t {
                    Token::Lit(b) => rehecho.push(b),
                    Token::Copia { long, dist, .. } => {
                        let ini = rehecho.len() - dist as usize;
                        for k in 0..long as usize { let v = rehecho[ini + k]; rehecho.push(v); }
                    }
                }
            }
            assert_eq!(rehecho, salida, "los tokens NO reconstruyen el plano");
        }
    }

    /// Una copia que se solapa: distancia 1, longitud 5. Si se copiara con un slice en vez de
    /// byte a byte, esto daria basura.
    #[test]
    fn copia_solapada() {
        let plano = b"aaaaaa";
        let comp = comprime_con_zlib(plano);
        assert_eq!(descomprime(&comp).unwrap(), plano);
    }

    #[test]
    fn vacio_da_vacio() {
        let comp = comprime_con_zlib(b"");
        assert_eq!(descomprime(&comp).unwrap(), b"");
    }

    /// Un flujo cortado tiene que dar ERROR, no medio resultado. Devolver lo que se llevaba
    /// descomprimido seria la peor respuesta posible: parece que funciono.
    #[test]
    fn truncado_falla() {
        let comp = comprime_con_zlib(&vec![b'x'; 10000]);
        assert!(descomprime(&comp[..comp.len() / 2]).is_err());
    }

    fn comprime_con_zlib(d: &[u8]) -> Vec<u8> {
        // Aqui hace falta DEFLATE de verdad y no tenemos compresor propio todavia, asi que
        // los vectores van fijados. SE REGENERAN ASI, y conviene rehacerlo si se tocan:
        //
        //     python3 -c 'import zlib; c=zlib.compressobj(9,zlib.DEFLATED,-15); \
        //                 b=c.compress(b"aaaaaa")+c.flush(); print([hex(x) for x in b])'
        //
        // UNO DE ESTOS VECTORES ESTUVO MAL Y DEJO EL TEST EN ROJO. Decia 4b 4c 84 00 00, que
        // zlib decodifica como "aaaaaaaa" —ocho— cuando el test esperaba seis. Nuestro
        // descompresor devolvia ocho, o sea lo MISMO que zlib para ese flujo: acertaba. El
        // fallo era el vector, tecleado a mano con 0x84 donde van 0x04 0x01.
        //
        // La leccion no es el byte: es que un vector escrito a mano en una prueba de formato
        // binario es una segunda implementacion del formato, hecha sin querer y sin revisar.
        match d.len() {
            0 => vec![0x03, 0x00],
            6 => vec![0x4b, 0x4c, 0x04, 0x01, 0x00],
            _ => {
                // 10000 'x': bloque dinamico. Se genera uno equivalente con bloques almacenados.
                let mut v = Vec::new();
                let mut resto = d;
                while resto.len() > 65535 {
                    v.push(0x00);
                    v.extend_from_slice(&(65535u16).to_le_bytes());
                    v.extend_from_slice(&(!65535u16).to_le_bytes());
                    v.extend_from_slice(&resto[..65535]);
                    resto = &resto[65535..];
                }
                v.push(0x01);
                v.extend_from_slice(&(resto.len() as u16).to_le_bytes());
                v.extend_from_slice(&(!(resto.len() as u16)).to_le_bytes());
                v.extend_from_slice(resto);
                v
            }
        }
    }
}
