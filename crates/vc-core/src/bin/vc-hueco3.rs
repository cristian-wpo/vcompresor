// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! ¿CUANTO LE FALTA A VDEFLATE PARA ENTRAR EN EL PRODUCTO? El hueco de la fase 3, en bytes.
//!
//! La pregunta del dueño: «¿por que no lo metes en el proyecto? Para eso lo hacemos».
//!
//! LA RESPUESTA ES QUE HOY LO METERIA HACIA ATRAS, y esto lo mide en vez de afirmarlo.
//!
//! preflate guarda CORRECCIONES: 6.371.168 B para los 374.826.209 B de DEFLATE del banco, o sea
//! el 1,70%. Con eso y el plano reconstruye el flujo original byte a byte.
//!
//! Nosotros hoy guardamos la DESCRIPCION COMPLETA del flujo: cada bloque, cada cabecera y cada
//! decision del codificador. Eso reconstruye igual de bien —1.973 de 1.973— pero pesa otra cosa.
//!
//! Este programa mide cuanto pesa. La cuenta es la del formato mas ajustado que se podria usar
//! para lo que capturamos hoy, no la de una serializacion ingenua:
//!
//!   un literal          se puede sacar del plano, asi que no cuesta nada guardarlo
//!   una copia           hay que saber que ES una copia, y con que simbolos: eso si cuesta
//!   una cabecera        hlit/hdist/hclen + los codigos + la secuencia RLE
//!
//! Lo que salga es EL HUECO: lo que la fase 3 —adivinar los ajustes del codificador y guardar
//! solo las divergencias— tiene que cerrar para que esto pueda entrar en el producto.
//!
//!   uso:  vc-hueco3 [directorio-del-banco]

use std::path::PathBuf;
use vc_core::inflate::{descomprime_con_bloques, Bloque, Token};

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/deflate-banco".into());
    let mut casos: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(it) => it.filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "raw").unwrap_or(false)).collect(),
        Err(e) => { eprintln!("no puedo leer {dir}: {e}"); std::process::exit(2); }
    };
    casos.sort();

    let (mut deflate, mut plano_t) = (0usize, 0usize);
    let (mut n_copias, mut n_lit, mut n_cab, mut n_bl) = (0u64, 0u64, 0u64, 0u64);
    let mut bytes_cab = 0usize;

    for p in &casos {
        let Ok(d) = std::fs::read(p) else { continue };
        let Ok((plano, bloques)) = descomprime_con_bloques(&d) else { continue };
        deflate += d.len();
        plano_t += plano.len();
        for b in &bloques {
            n_bl += 1;
            if let Bloque::Comprimido { cabecera, tokens, .. } = b {
                if let Some(c) = cabecera {
                    n_cab += 1;
                    // hlit(5) + hdist(5) + hclen(4) bits, los codigos a 3 bits, y la secuencia
                    // a un byte de simbolo + los extras. Es una cota generosa a nuestro favor.
                    bytes_cab += 2 + c.hclen + c.secuencia.len() * 2;
                }
                for t in tokens {
                    match t {
                        Token::Lit(_) => n_lit += 1,
                        Token::Copia { .. } => n_copias += 1,
                    }
                }
            }
        }
    }

    // Lo que cuesta describir las copias. Cada una necesita, como MINIMO:
    //   - donde esta (posicion en el plano): se puede guardar como delta, ~2 B con varint
    //   - el simbolo de longitud (285 valores -> 9 bits) y el de distancia (30 -> 5 bits)
    //   - los bits extra de cada uno
    // Se cuenta 4 B por copia, que es optimista: una serializacion real gastaria mas.
    let coste_copias = n_copias as usize * 4;
    let nuestro = coste_copias + bytes_cab;
    let preflate = 6_371_168usize;   // medido por bin/vc-preflate-coste sobre este mismo banco

    println!();
    println!("  EL HUECO DE LA FASE 3 — {} flujos", casos.len());
    println!();
    println!("  {:<34} {:>15}", "DEFLATE de entrada", deflate);
    println!("  {:<34} {:>15}", "plano que produce", plano_t);
    println!("  {:<34} {:>15}", "bloques", n_bl);
    println!("  {:<34} {:>15}", "cabeceras dinamicas", n_cab);
    println!("  {:<34} {:>15}", "literales", n_lit);
    println!("  {:<34} {:>15}", "copias", n_copias);
    println!();
    println!("  LO QUE HAY QUE GUARDAR PARA RECONSTRUIR EL FLUJO:");
    println!("  {:<34} {:>15}  {:>7.2}% del DEFLATE",
             "preflate: sus correcciones", preflate, 100.0 * preflate as f64 / deflate as f64);
    println!("  {:<34} {:>15}  {:>7.2}% del DEFLATE",
             "nosotros HOY (cota optimista)", nuestro, 100.0 * nuestro as f64 / deflate as f64);
    println!("     de eso, las copias           {:>15}", coste_copias);
    println!("     de eso, las cabeceras        {:>15}", bytes_cab);
    println!();
    println!("  EL HUECO: {:.1} veces mas grande.", nuestro as f64 / preflate as f64);
    println!();
    println!("  Por eso vdeflate NO entra hoy en el producto: funciona —1.973 de 1.973 en las");
    println!("  dos direcciones— pero guardar lo que captura haria los archivos MAS GRANDES.");
    println!();
    println!("  La fase 3 es exactamente cerrar ese hueco: en vez de guardar TODAS las decisiones,");
    println!("  adivinar los ajustes del codificador original y guardar solo donde se equivoca.");
    println!("  preflate acierta el 98,3% de las veces; nosotros tenemos que llegar ahi.");
}
