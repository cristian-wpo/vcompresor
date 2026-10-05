// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! DEFLATE propio, fase 2: VOLVER A EMITIR el flujo, byte a byte.
//!
//! POR QUE ESTA FASE Y POR QUE AHORA. El desglose directo del 2026-07-30 dice que preflate es el
//! **78% del reloj de todo el archivador**, y ese 78% se reparte asi:
//!
//! ```text
//! inflar                   9,6%    ya es nuestro (fase 1, 1.973/1.973 identicos)
//! estimar parametros      66,5%    la fase 3
//! re-deflate y verificar  23,9%    ESTO
//! ```
//!
//! El re-deflate no es opcional ni es solo para la fase 3: el camino de hoy ya lo corre SIEMPRE
//! —`preflate_seguro` recompone el flujo y lo compara byte a byte antes de aceptarlo, «paranoia:
//! si no es byte-exacto, NO entra»— asi que su coste esta dentro del 78% que pagamos hoy.
//!
//! LO QUE HACE DIFICIL ESTO, y no es la compresion. Un DEFLATE no tiene UNA codificacion: tiene
//! muchas igual de legales, y hay que devolver EXACTAMENTE la que habia.
//!
//!   · cuatro ceros en la tabla de longitudes caben en cuatro simbolos «0» o en un «17» con n=4
//!   · la longitud 258 admite el codigo 284 con extra=31 y tambien el 285
//!   · los bloques almacenados llevan relleno hasta el byte, y su frontera importa
//!
//! Por eso este modulo NO decide nada: replica. Toda la decision esta capturada en `Bloque` y
//! `Cabecera` por el inflate, tal cual venia. Un emisor que «recomprima bien» produciria un
//! DEFLATE valido y equivocado, que es el peor resultado posible aqui: pasa las pruebas de
//! formato y rompe la ida y vuelta.
//!
//! EL CONTRATO, y es el mismo de la fase 1: sobre los 1.973 flujos reales del banco, inflar y
//! volver a emitir tiene que dar LOS MISMOS BYTES. No hay nota parcial.

use crate::inflate::{Bloque, Cabecera, Token};
use anyhow::{bail, Result};

/// Escritor de bits en el orden de DEFLATE: del menos significativo al mas.
///
/// Es el espejo exacto de `inflate::Bits`. Se escribe aparte a proposito: compartir el estado
/// entre lector y escritor ahorraria treinta lineas y haria imposible probar uno contra otro.
struct Escritor {
    v: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Escritor {
    fn nuevo() -> Escritor {
        Escritor { v: Vec::new(), acc: 0, n: 0 }
    }

    /// Escribe `n` bits de `x`, del menos significativo al mas.
    fn bits(&mut self, x: u32, n: u32) {
        debug_assert!(n <= 24 && (n == 32 || x < (1u32 << n)));
        self.acc |= x << self.n;
        self.n += n;
        while self.n >= 8 {
            self.v.push((self.acc & 0xFF) as u8);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    /// Un codigo de Huffman va del bit MAS significativo al menos: al reves que todo lo demas
    /// en DEFLATE. Confundir los dos ordenes produce un flujo que casi funciona.
    fn codigo(&mut self, cod: u16, longitud: u8) {
        for i in (0..longitud).rev() {
            self.bits(((cod >> i) & 1) as u32, 1);
        }
    }

    /// Rellena con ceros hasta el byte. Lo piden los bloques almacenados.
    fn alinea(&mut self) {
        if self.n != 0 {
            self.v.push((self.acc & 0xFF) as u8);
            self.acc = 0;
            self.n = 0;
        }
    }

    fn cierra(mut self) -> Vec<u8> {
        self.alinea();
        self.v
    }
}

/// Los codigos canonicos de un alfabeto, a partir de la longitud de cada simbolo.
///
/// Es el mismo algoritmo del RFC 1951 §3.2.2 que usa el lector para decodificar; aqui va en la
/// direccion contraria. Si las dos implementaciones discrepan, la ida y vuelta lo caza.
fn codigos_canonicos(longs: &[u8]) -> Result<Vec<u16>> {
    let mut conteo = [0u16; 16];
    for &l in longs {
        if l as usize > 15 {
            bail!("longitud de codigo {} fuera de rango", l)
        }
        conteo[l as usize] += 1;
    }
    conteo[0] = 0;
    let mut siguiente = [0u16; 16];
    let mut codigo = 0u16;
    for l in 1..16 {
        codigo = (codigo + conteo[l - 1]) << 1;
        siguiente[l] = codigo;
    }
    let mut cods = vec![0u16; longs.len()];
    for (s, &l) in longs.iter().enumerate() {
        if l != 0 {
            cods[s] = siguiente[l as usize];
            siguiente[l as usize] += 1;
        }
    }
    Ok(cods)
}

const LONG_EXTRA: [u8; 29] = [0,0,0,0,0,0,0,0,1,1,1,1,2,2,2,2,3,3,3,3,4,4,4,4,5,5,5,5,0];
const LONG_BASE: [u16; 29] = [3,4,5,6,7,8,9,10,11,13,15,17,19,23,27,31,35,43,51,59,67,83,99,115,131,163,195,227,258];
const DIST_EXTRA: [u8; 30] = [0,0,0,0,1,1,2,2,3,3,4,4,5,5,6,6,7,7,8,8,9,9,10,10,11,11,12,12,13,13];
const DIST_BASE: [u16; 30] = [1,2,3,4,5,7,9,13,17,25,33,49,65,97,129,193,257,385,513,769,1025,1537,2049,3073,4097,6145,8193,12289,16385,24577];
const ORDEN_CL: [usize; 19] = [16,17,18,0,8,7,9,6,10,5,11,4,12,3,13,2,14,1,15];

/// Las longitudes de los arboles fijos del RFC (tipo de bloque 01).
fn longs_fijos() -> ([u8; 288], [u8; 30]) {
    let mut l = [0u8; 288];
    for (i, v) in l.iter_mut().enumerate() {
        *v = if i < 144 { 8 } else if i < 256 { 9 } else if i < 280 { 7 } else { 8 };
    }
    (l, [5u8; 30])
}

/// Reconstruye el array de longitudes a partir de la secuencia RLE capturada.
///
/// Se replica la secuencia en vez de recalcularla porque el mismo array admite varias
/// codificaciones legales y hay que devolver la que estaba.
fn longs_desde_secuencia(c: &Cabecera) -> Result<Vec<u8>> {
    let total = c.hlit + c.hdist;
    let mut longs = vec![0u8; total];
    let mut i = 0usize;
    for &(s, e) in &c.secuencia {
        match s {
            0..=15 => {
                if i >= total { bail!("la secuencia se sale") }
                longs[i] = s;
                i += 1;
            }
            16 => {
                if i == 0 { bail!("repeticion 16 sin anterior") }
                let v = longs[i - 1];
                let n = 3 + e as usize;
                if i + n > total { bail!("repeticion 16 se sale") }
                for _ in 0..n { longs[i] = v; i += 1; }
            }
            17 => {
                let n = 3 + e as usize;
                if i + n > total { bail!("repeticion 17 se sale") }
                i += n;
            }
            18 => {
                let n = 11 + e as usize;
                if i + n > total { bail!("repeticion 18 se sale") }
                i += n;
            }
            _ => bail!("simbolo {} imposible en el alfabeto de longitudes", s),
        }
    }
    if i != total {
        bail!("la secuencia cubre {} longitudes y hacen falta {}", i, total)
    }
    Ok(longs)
}

/// Vuelve a emitir el flujo DEFLATE descrito por `bloques`.
///
/// El contrato: si `bloques` viene de `inflate::descomprime_con_bloques`, esto tiene que
/// devolver EXACTAMENTE los bytes de entrada. No «un flujo equivalente»: los mismos.
pub fn emite(bloques: &[Bloque]) -> Result<Vec<u8>> {
    if bloques.is_empty() {
        bail!("no hay bloques que emitir")
    }
    let mut w = Escritor::nuevo();
    let (fl, fd) = longs_fijos();

    for b in bloques {
        match b {
            Bloque::Almacenado { ultimo, datos } => {
                w.bits(*ultimo as u32, 1);
                w.bits(0, 2);
                w.alinea();
                let len = datos.len();
                if len > 0xFFFF { bail!("bloque almacenado de {} bytes, y el tope es 65.535", len) }
                w.bits((len & 0xFF) as u32, 8);
                w.bits((len >> 8) as u32, 8);
                let n = !(len as u16);
                w.bits((n & 0xFF) as u32, 8);
                w.bits((n >> 8) as u32, 8);
                for &x in datos {
                    w.bits(x as u32, 8);
                }
            }
            Bloque::Comprimido { ultimo, cabecera, tokens } => {
                w.bits(*ultimo as u32, 1);
                let (longs_lit, longs_dist): (Vec<u8>, Vec<u8>) = match cabecera {
                    None => {
                        w.bits(1, 2);
                        (fl.to_vec(), fd.to_vec())
                    }
                    Some(c) => {
                        w.bits(2, 2);
                        w.bits((c.hlit - 257) as u32, 5);
                        w.bits((c.hdist - 1) as u32, 5);
                        w.bits((c.hclen - 4) as u32, 4);
                        for i in 0..c.hclen {
                            w.bits(c.cl[ORDEN_CL[i]] as u32, 3);
                        }
                        // La secuencia RLE va comprimida a su vez con el alfabeto de longitudes.
                        let cod_cl = codigos_canonicos(&c.cl)?;
                        for &(s, e) in &c.secuencia {
                            w.codigo(cod_cl[s as usize], c.cl[s as usize]);
                            match s {
                                16 => w.bits(e as u32, 2),
                                17 => w.bits(e as u32, 3),
                                18 => w.bits(e as u32, 7),
                                _ => {}
                            }
                        }
                        let longs = longs_desde_secuencia(c)?;
                        (longs[..c.hlit].to_vec(), longs[c.hlit..].to_vec())
                    }
                };
                let cod_lit = codigos_canonicos(&longs_lit)?;
                let cod_dist = codigos_canonicos(&longs_dist)?;

                for t in tokens {
                    match *t {
                        Token::Lit(x) => {
                            let s = x as usize;
                            if longs_lit[s] == 0 { bail!("el literal {} no tiene codigo", x) }
                            w.codigo(cod_lit[s], longs_lit[s]);
                        }
                        Token::Copia { long, dist, sim_long, sim_dist } => {
                            // Se usa EL SIMBOLO CAPTURADO, no el que se derivaria de la
                            // longitud: la 258 admite el 284 con extra=31 y tambien el 285.
                            let sl = sim_long as usize;
                            if sl < 257 || sl > 285 { bail!("simbolo de longitud {} invalido", sl) }
                            if longs_lit[sl] == 0 { bail!("el simbolo de longitud {} no tiene codigo", sl) }
                            w.codigo(cod_lit[sl], longs_lit[sl]);
                            let i = sl - 257;
                            let extra = LONG_EXTRA[i];
                            if extra > 0 {
                                let v = long as u32 - LONG_BASE[i] as u32;
                                w.bits(v, extra as u32);
                            }
                            let sd = sim_dist as usize;
                            if sd >= 30 { bail!("simbolo de distancia {} invalido", sd) }
                            if longs_dist[sd] == 0 { bail!("el simbolo de distancia {} no tiene codigo", sd) }
                            w.codigo(cod_dist[sd], longs_dist[sd]);
                            let de = DIST_EXTRA[sd];
                            if de > 0 {
                                let v = dist as u32 - DIST_BASE[sd] as u32;
                                w.bits(v, de as u32);
                            }
                        }
                    }
                }
                // el 256 cierra el bloque
                if longs_lit[256] == 0 { bail!("el simbolo de fin de bloque no tiene codigo") }
                w.codigo(cod_lit[256], longs_lit[256]);
            }
        }
    }
    Ok(w.cierra())
}

#[cfg(test)]
mod pruebas {
    use super::*;
    use crate::inflate::descomprime_con_bloques;

    /// La prueba que importa: inflar y volver a emitir tiene que dar LOS MISMOS BYTES.
    ///
    /// Los vectores salen de zlib de verdad. Se regeneran asi:
    ///   python3 -c 'import zlib; c=zlib.compressobj(9,zlib.DEFLATED,-15); \
    ///               print([hex(x) for x in c.compress(b"...")+c.flush()])'
    fn ida_y_vuelta(comp: &[u8]) {
        let (plano, bloques) = descomprime_con_bloques(comp).expect("inflar");
        let rehecho = emite(&bloques).expect("emitir");
        assert_eq!(rehecho, comp, "los bytes no son los mismos (plano de {} B)", plano.len());
    }

    #[test]
    fn almacenado() {
        ida_y_vuelta(&[0x01, 0x05, 0x00, 0xFA, 0xFF, b'h', b'o', b'l', b'a', b'!']);
    }

    #[test]
    fn copia_solapada() {
        // zlib de "aaaaaa": arbol fijo con una copia de distancia 1
        ida_y_vuelta(&[0x4b, 0x4c, 0x04, 0x01, 0x00]);
    }

    #[test]
    fn vacio() {
        ida_y_vuelta(&[0x03, 0x00]);
    }

    /// EL UNICO HUECO DE COBERTURA DEL BANCO, cerrado a mano.
    ///
    /// La longitud 258 tiene dos codificaciones legales: el codigo 285, y el 284 con extra=31
    /// (base 227 + 31 = 258). En los 1.973 flujos reales aparece 17.243.340 veces como 285 y
    /// CERO como 284+31, porque zlib usa siempre el 285. Esa rama no se habia ejecutado nunca.
    ///
    /// Se fabrica el caso: un bloque de arboles fijos con una copia de longitud 258 y distancia
    /// 1, forzando el simbolo 284. Se comprueba que nuestro inflate lo lee como 258, que el
    /// plano sale bien, y que re-emitirlo preserva el 284 en vez de «corregirlo» a 285.
    #[test]
    fn longitud_258_como_284_mas_31() {
        use crate::inflate::{descomprime, descomprime_con_bloques};
        // 'a' literal, luego copia de 258 bytes a distancia 1 -> 259 aes
        let bl = vec![Bloque::Comprimido {
            ultimo: true,
            cabecera: None,   // arboles fijos
            tokens: vec![
                Token::Lit(b'a'),
                Token::Copia { long: 258, dist: 1, sim_long: 284, sim_dist: 0 },
            ],
        }];
        let flujo = emite(&bl).expect("emitir con 284");

        // 1. nuestro inflate lo lee bien
        let plano = descomprime(&flujo).expect("nuestro inflate");
        assert_eq!(plano.len(), 259, "la longitud 258 via 284 no se leyo bien");
        assert!(plano.iter().all(|&x| x == b'a'));

        // 2. y el SIMBOLO se preserva al volver a emitir: si el emisor «corrigiera» a 285,
        //    los bytes cambiarian y la ida y vuelta de un archivo real se romperia.
        let (_p, bl2) = descomprime_con_bloques(&flujo).expect("releer");
        let rehecho = emite(&bl2).expect("re-emitir");
        assert_eq!(rehecho, flujo, "el simbolo 284 no sobrevivio a la ida y vuelta");
        match &bl2[0] {
            Bloque::Comprimido { tokens, .. } => match tokens[1] {
                Token::Copia { sim_long, long, .. } => {
                    assert_eq!(sim_long, 284, "se leyo con otro simbolo");
                    assert_eq!(long, 258);
                }
                _ => panic!("se esperaba una copia"),
            },
            _ => panic!("se esperaba un bloque comprimido"),
        }
    }
}
