// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! Spike F2.2: preflate-rs sobre .gz REALES — plaintext + correcciones -> recrear deflate byte-exacto.
//! Mide la ganancia de archivado: zstd19(plaintext)+correcciones vs zstd19(deflate original).

use anyhow::{bail, Context, Result};
use preflate_rs::{preflate_whole_deflate_stream, recreate_whole_deflate_stream, PreflateConfig};

fn gzip_deflate_body(gz: &[u8]) -> Result<(usize, usize)> {
    if gz.len() < 20 || gz[0] != 0x1f || gz[1] != 0x8b || gz[2] != 8 { bail!("no es gzip/deflate"); }
    let flg = gz[3];
    let mut p = 10usize;
    if flg & 4 != 0 { let xlen = u16::from_le_bytes([gz[p], gz[p+1]]) as usize; p += 2 + xlen; }
    if flg & 8 != 0 { while gz[p] != 0 { p += 1; } p += 1; }   // FNAME
    if flg & 16 != 0 { while gz[p] != 0 { p += 1; } p += 1; }  // FCOMMENT
    if flg & 2 != 0 { p += 2; }                                 // FHCRC
    Ok((p, gz.len() - 8))
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut total = [0u64; 5]; // orig_deflate, plain, corr, plain_zst+corr, deflate_zst
    let mut n = 0u32;
    for path in args.by_ref() {
        let gz = std::fs::read(&path).with_context(|| path.clone())?;
        let (a, b) = match gzip_deflate_body(&gz) { Ok(x) => x, Err(e) => { eprintln!("SKIP {path}: {e}"); continue; } };
        let body = &gz[a..b];
        let t0 = std::time::Instant::now();
        let (res, plain) = match preflate_whole_deflate_stream(body, &PreflateConfig::default()) {
            Ok(x) => x, Err(e) => { eprintln!("STORE {path}: preflate no pudo ({e})"); continue; }
        };
        let recreated = recreate_whole_deflate_stream(plain.text(), &res.corrections)?;
        let exacto = recreated == body[..res.compressed_size];
        let pz = zstd::bulk::compress(plain.text(), 19)?.len();
        let dz = zstd::bulk::compress(body, 19)?.len();
        let ours = pz + res.corrections.len();
        println!("{path}: deflate={} plain={} corr={} | zstd(plain)+corr={} vs zstd(deflate)={} | gana={:.1}% exacto={} {:?}",
            body.len(), plain.text().len(), res.corrections.len(), ours, dz,
            100.0 * (1.0 - ours as f64 / dz as f64), exacto, t0.elapsed());
        if !exacto { bail!("RECONSTRUCCION NO EXACTA en {path} — parar todo"); }
        total[0] += body.len() as u64; total[1] += plain.text().len() as u64; total[2] += res.corrections.len() as u64;
        total[3] += ours as u64; total[4] += dz as u64;
        n += 1;
    }
    if n > 0 {
        println!("\nTOTAL n={n}: deflate={} plain={} corr={} | nuestro={} vs zstd(deflate)={} | GANANCIA={:.2}%",
            total[0], total[1], total[2], total[3], total[4],
            100.0 * (1.0 - total[3] as f64 / total[4] as f64));
    }
    Ok(())
}
