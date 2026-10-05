// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! EL CONSTRUCTOR DEL BANCO DE DEFLATE, que hasta hoy no existía.
//!
//! POR QUÉ EXISTE, y no es orden: el banco `/tmp/deflate-banco` lo hizo un guion suelto que ya
//! no está, y por eso nadie sabía qué había dentro. Al cruzar los resultados de la fase 3 con el
//! censo por tipo de fichero salió que **los 1.973 flujos son 1.973 PNG**. Ni un zip, ni un
//! gzip, ni un DEFLATE de PDF o de office.
//!
//! O sea que el 87,71% de acierto, el 91,24% del oráculo y los +27,50 puntos de la política de
//! diccionario están medidos sobre IDAT y solo sobre IDAT. No están mal — están **acotados**, y
//! hasta hoy nadie lo había escrito porque el instrumento que fabricaba el corpus no vivía en
//! ningún sitio donde se pudiera leer.
//!
//! Es exactamente la lección de «un barrido vale lo que vale su corpus», aplicada un nivel más
//! abajo: el corpus también es un instrumento, y un instrumento que no está en el repo no se
//! puede auditar.
//!
//! ⚠️ Y no es un detalle de cobertura: PNG es el caso MÁS FÁCIL de todos. Sus IDAT los produce
//! casi siempre la misma familia de codificadores (libpng sobre zlib, o libdeflate), con
//! parámetros por defecto y sobre datos ya filtrados. Un DOCX lo escribió el zip de quien fuera;
//! un PDF, la biblioteca de PDF de quien fuera. Justo la variedad que la fase 3 tiene que
//! aguantar y que el banco no le estaba enseñando.
//!
//!   uso:  vc-banco-deflate <directorio-de-entrada> <directorio-de-salida>
//!
//! Escribe `<contenedor>-<huella>.raw` (el flujo DEFLATE crudo) y `.plano` (lo que sale al
//! inflarlo). SOLO escribe los que inflan de verdad: un flujo que no se puede verificar no entra
//! al banco, porque un fallo del extractor saldría contado como un fallo del predictor.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use vc_core::inflate::descomprime;

/// Un flujo DEFLATE crudo sacado de un contenedor, con de dónde salió.
struct Flujo {
    contenedor: &'static str,
    crudo: Vec<u8>,
}

/// PNG: los IDAT son UN solo flujo zlib partido en trozos, así que hay que concatenarlos ANTES
/// de inflar. Partirlos por chunk y tratar cada uno como un flujo daría basura.
fn de_png(d: &[u8]) -> Vec<Flujo> {
    if d.len() < 8 || &d[..8] != b"\x89PNG\r\n\x1a\n" { return vec![] }
    let mut idat = Vec::new();
    let mut p = 8usize;
    while p + 8 <= d.len() {
        let n = u32::from_be_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]]) as usize;
        let tipo = &d[p + 4..p + 8];
        let ini = p + 8;
        if ini + n + 4 > d.len() { break }
        if tipo == b"IDAT" { idat.extend_from_slice(&d[ini..ini + n]); }
        if tipo == b"IEND" { break }
        p = ini + n + 4;
    }
    // cabecera zlib de 2 bytes + adler de 4 al final
    if idat.len() > 6 {
        vec![Flujo { contenedor: "png", crudo: idat[2..idat.len() - 4].to_vec() }]
    } else { vec![] }
}

/// ZIP (y con él docx, xlsx, pptx, odt, jar…). Se lee el DIRECTORIO CENTRAL y no las cabeceras
/// locales, y es a propósito: cuando el zip se escribió en streaming, la cabecera local lleva
/// los tamaños a cero y los de verdad van en un descriptor DETRÁS de los datos. El directorio
/// central siempre los tiene bien.
fn de_zip(d: &[u8]) -> Vec<Flujo> {
    if d.len() < 4 || &d[..4] != b"PK\x03\x04" { return vec![] }
    // el fin del directorio central está al final, con hasta 65.535 bytes de comentario detrás
    let ini = d.len().saturating_sub(65_557);
    let Some(eocd) = (ini..d.len().saturating_sub(21))
        .rev().find(|&i| &d[i..i + 4] == b"PK\x05\x06") else { return vec![] };
    let n = u16::from_le_bytes([d[eocd + 10], d[eocd + 11]]) as usize;
    let mut off = u32::from_le_bytes([d[eocd + 16], d[eocd + 17], d[eocd + 18], d[eocd + 19]]) as usize;
    let mut out = Vec::new();
    for _ in 0..n {
        if off + 46 > d.len() || &d[off..off + 4] != b"PK\x01\x02" { break }
        let metodo = u16::from_le_bytes([d[off + 10], d[off + 11]]);
        let comp = u32::from_le_bytes([d[off + 20], d[off + 21], d[off + 22], d[off + 23]]) as usize;
        let nl = u16::from_le_bytes([d[off + 28], d[off + 29]]) as usize;
        let el = u16::from_le_bytes([d[off + 30], d[off + 31]]) as usize;
        let cl = u16::from_le_bytes([d[off + 32], d[off + 33]]) as usize;
        let lho = u32::from_le_bytes([d[off + 42], d[off + 43], d[off + 44], d[off + 45]]) as usize;
        // 8 = deflate. 0 es «guardado» y no nos sirve; 9/12/14 son otros y tampoco.
        if metodo == 8 && lho + 30 <= d.len() && &d[lho..lho + 4] == b"PK\x03\x04" {
            let lnl = u16::from_le_bytes([d[lho + 26], d[lho + 27]]) as usize;
            let lel = u16::from_le_bytes([d[lho + 28], d[lho + 29]]) as usize;
            let ini = lho + 30 + lnl + lel;
            if comp > 0 && ini + comp <= d.len() {
                out.push(Flujo { contenedor: "zip", crudo: d[ini..ini + comp].to_vec() });
            }
        }
        off += 46 + nl + el + cl;
    }
    out
}

/// GZIP: cabecera de 10 bytes más lo que digan las banderas, y 8 de cola (CRC + tamaño).
fn de_gz(d: &[u8]) -> Vec<Flujo> {
    if d.len() < 18 || d[0] != 0x1f || d[1] != 0x8b || d[2] != 8 { return vec![] }
    let f = d[3];
    let mut p = 10usize;
    if f & 4 != 0 {                                   // FEXTRA
        if p + 2 > d.len() { return vec![] }
        p += 2 + u16::from_le_bytes([d[p], d[p + 1]]) as usize;
    }
    for bit in [8u8, 16] {                            // FNAME, FCOMMENT: cadenas con cero al final
        if f & bit != 0 {
            while p < d.len() && d[p] != 0 { p += 1 }
            p += 1;
        }
    }
    if f & 2 != 0 { p += 2 }                          // FHCRC
    if p + 8 >= d.len() { return vec![] }
    vec![Flujo { contenedor: "gz", crudo: d[p..d.len() - 8].to_vec() }]
}

/// PDF: los flujos con `/FlateDecode` entre `stream` y `endstream`.
///
/// ⚠️ El `/Length` del diccionario puede ser una referencia indirecta a otro objeto, así que NO
/// se puede usar sin resolver la tabla de referencias cruzadas. Se busca el `endstream` y se
/// recorta la cola en blanco; si el recorte deja algo que no infla, el filtro final lo tira.
/// ⚠️ Y los PDF cifrados no inflan: se caen solos en la verificación, que es donde deben caerse.
fn de_pdf(d: &[u8]) -> Vec<Flujo> {
    if d.len() < 5 || &d[..5] != b"%PDF-" { return vec![] }
    let mut out = Vec::new();
    let mut p = 0usize;
    while let Some(r) = busca(d, b"stream", p) {
        // que el diccionario de justo antes diga FlateDecode; 512 B hacia atrás sobra
        let atras = r.saturating_sub(512);
        let dic = &d[atras..r];
        let es_flate = busca(dic, b"/FlateDecode", 0).is_some();
        let mut ini = r + 6;
        if d.get(ini) == Some(&b'\r') { ini += 1 }
        if d.get(ini) == Some(&b'\n') { ini += 1 }
        let Some(fin) = busca(d, b"endstream", ini) else { break };
        if es_flate && fin > ini {
            let mut z = &d[ini..fin];
            while z.last().is_some_and(|c| *c == b'\n' || *c == b'\r') { z = &z[..z.len() - 1] }
            // cabecera zlib de 2 bytes: 0x?8 con el chequeo cuadrando
            if z.len() > 8 && z[0] & 0x0F == 8 && (z[0] as u16 * 256 + z[1] as u16) % 31 == 0 {
                out.push(Flujo { contenedor: "pdf", crudo: z[2..].to_vec() });
            }
        }
        p = fin + 9;
    }
    out
}

fn busca(h: &[u8], a: &[u8], desde: usize) -> Option<usize> {
    if desde >= h.len() || a.is_empty() { return None }
    h[desde..].windows(a.len()).position(|w| w == a).map(|i| i + desde)
}

fn main() {
    let mut arg = std::env::args().skip(1);
    let ent = arg.next().unwrap_or_else(|| "/tmp/corpus-hilos".into());
    let sal = arg.next().unwrap_or_else(|| "/tmp/deflate-banco2".into());
    let min: usize = std::env::var("VC_BANCO_MIN").ok().and_then(|v| v.parse().ok()).unwrap_or(256);
    std::fs::create_dir_all(&sal).expect("no puedo crear el directorio de salida");

    let mut ficheros: Vec<PathBuf> = Vec::new();
    anda(Path::new(&ent), &mut ficheros);
    ficheros.sort();
    eprintln!("  {} ficheros que mirar en {}", ficheros.len(), ent);

    let mut vistos: HashSet<[u8; 16]> = HashSet::new();
    let (mut n_ok, mut n_falla, mut n_dup, mut bytes) = (0u64, 0u64, 0u64, 0u64);
    let mut por_cont: std::collections::BTreeMap<&str, (u64, u64)> = Default::default();

    for f in &ficheros {
        let Ok(d) = std::fs::read(f) else { continue };
        let mut flujos = de_png(&d);
        if flujos.is_empty() { flujos = de_zip(&d) }
        if flujos.is_empty() { flujos = de_gz(&d) }
        if flujos.is_empty() { flujos = de_pdf(&d) }
        for fl in flujos {
            if fl.crudo.len() < min { continue }
            // LA VERIFICACIÓN, y por eso está aquí y no en el que mide: un flujo que no infla es
            // un fallo DEL EXTRACTOR, y si entrara al banco saldría contado como un fallo del
            // predictor. Que la frontera entre los dos instrumentos esté en el sitio correcto es
            // lo que hace que un número del banco signifique lo que dice que significa.
            let Ok(plano) = descomprime(&fl.crudo) else { n_falla += 1; continue };
            if plano.is_empty() { n_falla += 1; continue }
            let h = blake3::hash(&fl.crudo);
            let mut k = [0u8; 16];
            k.copy_from_slice(&h.as_bytes()[..16]);
            if !vistos.insert(k) { n_dup += 1; continue }
            let nom = format!("{}-{}", fl.contenedor, hex(&k[..8]));
            let _ = std::fs::write(format!("{sal}/{nom}.raw"), &fl.crudo);
            let _ = std::fs::write(format!("{sal}/{nom}.plano"), &plano);
            let e = por_cont.entry(fl.contenedor).or_default();
            e.0 += 1;
            e.1 += fl.crudo.len() as u64;
            bytes += fl.crudo.len() as u64;
            n_ok += 1;
        }
    }

    println!("\n  BANCO EN {sal}");
    println!("  {:<10} {:>8} {:>16} {:>9}", "contenedor", "flujos", "bytes DEFLATE", "% bytes");
    println!("  {}", "-".repeat(48));
    for (c, (n, b)) in &por_cont {
        println!("  {:<10} {:>8} {:>16} {:>8.2}%", c, n, con_puntos(*b),
                 if bytes > 0 { 100.0 * *b as f64 / bytes as f64 } else { 0.0 });
    }
    println!("  {}", "-".repeat(48));
    println!("  {:<10} {:>8} {:>16} {:>8.2}%", "TOTAL", n_ok, con_puntos(bytes), 100.0);
    println!("\n  descartados: {n_falla} que no inflan (extractor o cifrado) · {n_dup} repetidos");
    if por_cont.len() < 2 {
        println!("\n  ⚠️ UN SOLO CONTENEDOR. Es justo lo que le pasaba al banco viejo y por lo que");
        println!("     existe esta herramienta: todo lo que se mida aquí queda acotado a él.");
    }
}

fn anda(d: &Path, out: &mut Vec<PathBuf>) {
    let Ok(it) = std::fs::read_dir(d) else { return };
    for e in it.flatten() {
        let p = e.path();
        if p.is_dir() { anda(&p, out) } else if p.is_file() { out.push(p) }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn con_puntos(n: u64) -> String {
    let s = n.to_string();
    let mut o = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 { o.push('.') }
        o.push(c);
    }
    o
}
