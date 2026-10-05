// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
//! EL ARNES DE ACEPTACION del DEFLATE propio.
//!
//! Corre los flujos reales del banco (bench/saca-deflate.py) y compara lo que saca nuestro
//! descompresor contra lo que saco zlib, BYTE A BYTE. No hay nota parcial: o sale identico o no.
//!
//! POR QUE ESTE ARNES Y NO UNAS PRUEBAS UNITARIAS. Las unitarias del modulo cubren los casos que
//! se me ocurrieron a mi, y esos son justo los que ya tengo bien. Los 2.676 flujos de aqui los
//! escribieron libpng, zlib y quien sea que produjera esos PNG, con ajustes que nadie eligio
//! pensando en nosotros. Esa es la unica clase de entrada que sirve para decir «funciona».
//!
//! Uso:  vc-inflate [directorio-del-banco]     (por defecto /tmp/deflate-banco)

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/deflate-banco".into());
    let mut casos: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(it) => it.filter_map(|e| e.ok().map(|e| e.path()))
                    .filter(|p| p.extension().map(|e| e == "raw").unwrap_or(false))
                    .collect(),
        Err(e) => { eprintln!("no puedo leer {dir}: {e}"); std::process::exit(2); }
    };
    casos.sort();   // mismo orden siempre: dos ejecuciones tienen que dar el mismo informe
    if casos.is_empty() {
        eprintln!("el banco esta vacio. Corre antes: python3 bench/saca-deflate.py");
        std::process::exit(2);
    }

    let (mut ok, mut mal, mut roto) = (0usize, 0usize, 0usize);
    let (mut bytes_in, mut bytes_out) = (0u64, 0u64);
    let mut fallos: Vec<String> = Vec::new();
    let t0 = Instant::now();

    for p in &casos {
        let crudo = match std::fs::read(p) { Ok(v) => v, Err(_) => continue };
        let esperado = match std::fs::read(p.with_extension("plano")) { Ok(v) => v, Err(_) => continue };
        bytes_in += crudo.len() as u64;
        match vc_core::inflate::descomprime(&crudo) {
            Ok(v) => {
                bytes_out += v.len() as u64;
                if v == esperado { ok += 1; }
                else {
                    mal += 1;
                    // DONDE se separa, no solo que se separa: el primer byte distinto dice
                    // muchisimo mas que un «no coincide».
                    let d = v.iter().zip(esperado.iter()).position(|(a, b)| a != b);
                    fallos.push(format!("{}: difiere en el byte {:?} ({} B contra {} B)",
                        p.file_name().unwrap().to_string_lossy(), d, v.len(), esperado.len()));
                }
            }
            Err(e) => {
                roto += 1;
                fallos.push(format!("{}: {}", p.file_name().unwrap().to_string_lossy(), e));
            }
        }
    }

    let s = t0.elapsed().as_secs_f64();
    let n = casos.len();
    println!("ARNES DE ACEPTACION — DEFLATE propio, fase 1 (descomprimir)");
    println!("  flujos reales:   {n}");
    println!("  IDENTICOS:       {ok}  ({:.2}%)", 100.0 * ok as f64 / n as f64);
    println!("  distintos:       {mal}");
    println!("  con error:       {roto}");
    println!("  {:.1} MiB comprimidos -> {:.1} MiB planos en {:.1} s ({:.1} MB/s de salida)",
        bytes_in as f64 / 1048576.0, bytes_out as f64 / 1048576.0, s,
        bytes_out as f64 / 1048576.0 / s.max(0.001));
    if !fallos.is_empty() {
        println!("\n  los primeros {} fallos:", fallos.len().min(15));
        for f in fallos.iter().take(15) { println!("    {f}"); }
    }
    println!("\n  {}", if ok == n { format!("PASA: los {n} salen identicos.") }
                        else { "NO PASA. Mientras esto no este a cero, el resto del proyecto no empieza.".to_string() });
    // LA PIRAMIDE: cuenta exacta, no muestreo. Va ANTES del exit — la primera version quedo
    // detras y era codigo muerto que el compilador ni avisa por estar en un bin.
    if std::env::var("VC_INFLATE_CUENTA").is_ok() {
        println!();
        for l in vc_core::inflate::piramide(bytes_in as u64) { println!("{l}"); }
    }
    std::process::exit(if ok == n { 0 } else { 1 });
}
