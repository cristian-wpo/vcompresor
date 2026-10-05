// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! ¿QUE FRACCION DEL TIEMPO DE ZSTD ES BUSCAR COINCIDENCIAS? La prueba que abre D13.
//!
//! POR QUE IMPORTA. La piramide dice que «comprimir» son 133,28 s de los 197,46 s de CPU del
//! archivador entero: el 67,5%. Y el camino de hoy le da a zstd un plano de 6,03 GB que salio de
//! expandir 357 MB de DEFLATE — un DEFLATE que YA TRAIA las coincidencias encontradas, y que
//! nosotros tiramos. Si buscar coincidencias es la mitad del tiempo de zstd, hay un tercio del
//! CPU del programa entero encima de la mesa, y solo se puede coger teniendo el codec propio.
//!
//! Si es el 10%, D13 se cierra como idea bonita y cara, y se dice.
//!
//! COMO SE MIDE, sin creerse ninguna teoria. zstd es una biblioteca de C compilada dentro de
//! nuestro binario, asi que sus funciones aparecen con nombre en un perfil. Este programa no
//! hace mas que comprimir, para que el perfil sea suyo y de nadie mas; el reparto lo saca
//! `bench/zstd-busqueda.sh` con `perf record` y agrupando los simbolos por lo que hacen.
//!
//! Y ADEMAS UNA SEGUNDA MEDIDA QUE NO DEPENDE DE PERF, por si perf no esta o miente: la misma
//! entrada comprimida con las ESTRATEGIAS de zstd, de la que casi no busca (`fast`) a la que
//! busca de verdad (`btultra2`). Lo que cambia entre ellas es CASI SOLO el buscador, asi que la
//! diferencia de reloj acota lo que cuesta buscar. Dos caminos independientes al mismo numero.
//!
//!   uso:  vc-zstd-perfil <fichero-plano> [nivel]
//!         vc-zstd-perfil --prepara <dir-banco> <salida> <MB>   (fabrica el plano)

use std::io::Write;
use vc_core::inflate::descomprime;

fn main() {
    let a: Vec<String> = std::env::args().collect();

    // ── modo preparar: fabrica un trozo de plano real desde el banco ────────────────────────
    if a.len() >= 5 && a[1] == "--prepara" {
        let (dir, sal, mb) = (&a[2], &a[3], a[4].parse::<usize>().unwrap_or(512));
        let tope = mb * 1024 * 1024;
        let mut casos: Vec<std::path::PathBuf> = std::fs::read_dir(dir).unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "raw").unwrap_or(false)).collect();
        casos.sort();
        let mut out = std::fs::File::create(sal).unwrap();
        let mut n = 0usize;
        for p in &casos {
            let Ok(d) = std::fs::read(p) else { continue };
            let Ok(pl) = descomprime(&d) else { continue };
            let cuanto = pl.len().min(tope - n);
            out.write_all(&pl[..cuanto]).unwrap();
            n += cuanto;
            if n >= tope { break }
        }
        println!("{n} B de plano REAL escritos en {sal}");
        return;
    }

    let ruta = a.get(1).cloned().unwrap_or_else(|| "/tmp/plano.bin".into());
    let nivel: i32 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(19);
    let d = std::fs::read(&ruta).expect("no puedo leer el plano");

    // ── el perfil: SOLO comprimir, para que el perfil sea suyo ───────────────────────────────
    if std::env::var("VC_SOLO_COMPRIME").is_ok() {
        let t = std::time::Instant::now();
        let z = zstd::bulk::compress(&d, nivel).unwrap();
        eprintln!("  {} B -> {} B en {:.3} s ({:.1} MB/s)", d.len(), z.len(),
                  t.elapsed().as_secs_f64(), d.len() as f64 / 1e6 / t.elapsed().as_secs_f64());
        return;
    }

    // ── el barrido de estrategias: el camino que no depende de perf ─────────────────────────
    //
    // Lo que cambia entre estas siete es CASI SOLO cuanto se esfuerza el buscador de
    // coincidencias: la parte de entropia -Huffman de literales y FSE de las secuencias- es la
    // misma. Asi que la diferencia de reloj entre la que casi no busca y la que busca a fondo
    // ACOTA lo que cuesta buscar. No lo mide exacto, lo acota, y eso ya decide.
    use zstd::zstd_safe::zstd_sys::ZSTD_strategy::*;
    let estrategias = [
        ("fast     (casi no busca)", ZSTD_fast as i32),
        ("dfast",                    ZSTD_dfast as i32),
        ("greedy",                   ZSTD_greedy as i32),
        ("lazy",                     ZSTD_lazy as i32),
        ("lazy2",                    ZSTD_lazy2 as i32),
        ("btlazy2",                  ZSTD_btlazy2 as i32),
        ("btopt",                    ZSTD_btopt as i32),
        ("btultra",                  ZSTD_btultra as i32),
        ("btultra2 (busca a fondo)", ZSTD_btultra2 as i32),
    ];
    println!();
    println!("  ZSTD SOBRE {} B DE PLANO REAL, nivel {nivel}, cambiando SOLO la estrategia", d.len());
    println!("  (lo que cambia entre estas nueve es casi solo el BUSCADOR; la entropia es la misma)");
    println!();
    println!("  {:<28} {:>12} {:>10} {:>10}", "estrategia", "bytes", "seg", "MB/s");
    let mut primera = 0.0f64;
    for (nom, e) in estrategias {
        let mut c = zstd::bulk::Compressor::new(nivel).unwrap();
        if c.set_parameter(zstd::zstd_safe::CParameter::Strategy(
                unsafe { std::mem::transmute::<i32, zstd::zstd_safe::zstd_sys::ZSTD_strategy>(e) })).is_err() {
            continue;
        }
        let t = std::time::Instant::now();
        let Ok(z) = c.compress(&d) else { continue };
        let s = t.elapsed().as_secs_f64();
        if primera == 0.0 { primera = s }
        println!("  {:<28} {:>12} {:>10.3} {:>10.1}   {}", nom, z.len(), s,
                 d.len() as f64 / 1e6 / s,
                 if s > primera { format!("{:.1}x mas que fast", s / primera) } else { String::new() });
    }
    println!();
    println!("  COMO LEER ESTO. `fast` tambien busca —solo que poco—, asi que la diferencia entre");
    println!("  la primera y la ultima es una COTA INFERIOR de lo que cuesta buscar, no la cifra");
    println!("  exacta. Si ya esa cota es la mitad del tiempo, D13 esta abierto de par en par.");

    // ── LA PREGUNTA QUE DE VERDAD DECIDE D13 ────────────────────────────────────────────────
    //
    // Que buscar sea caro no basta. La idea de D13 es darle a zstd las coincidencias que el
    // DEFLATE YA TRAIA, y eso solo vale si se parecen a las que zstd encuentra por su cuenta.
    // Hay un motivo de peso para dudarlo: DEFLATE mira 32 KB hacia atras y zstd -19 mira 8 MB.
    // Si lo que zstd gana viene de mirar LEJOS, los tokens del DEFLATE no le sirven.
    //
    // Se mide limitando la ventana de zstd a la de DEFLATE y viendo que pierde.
    println!();
    println!("  ¿CUANTO DE LO QUE GANA ZSTD VIENE DE MIRAR MAS LEJOS QUE DEFLATE?");
    println!("  (DEFLATE mira 32 KB —windowLog 15—; zstd -19 mira 8 MB —windowLog 23—)");
    println!();
    println!("  {:<26} {:>12} {:>10} {:>10}", "ventana", "bytes", "seg", "MB/s");
    let mut base = 0usize;
    for wl in [15u32, 17, 19, 21, 23] {
        let mut c = zstd::bulk::Compressor::new(nivel).unwrap();
        if c.set_parameter(zstd::zstd_safe::CParameter::WindowLog(wl)).is_err() { continue }
        let t = std::time::Instant::now();
        let Ok(z) = c.compress(&d) else { continue };
        let sg = t.elapsed().as_secs_f64();
        if base == 0 { base = z.len() }
        let et = format!("{} KB (windowLog {})", (1u64 << wl) / 1024, wl);
        println!("  {:<26} {:>12} {:>10.3} {:>10.1}   {}", et, z.len(), sg,
                 d.len() as f64 / 1e6 / sg,
                 if z.len() < base { format!("{:.2}% mejor que 32 KB",
                     100.0 * (base - z.len()) as f64 / base as f64) } else { String::new() });
    }
    println!();
    println!("  COMO LEER ESTO, y es lo que decide D13. Si con la ventana de DEFLATE zstd casi no");
    println!("  pierde, sus coincidencias y las del DEFLATE son la misma cosa y darselas hechas");
    println!("  ahorra el >=99% del tiempo de buscar. Si pierde mucho, es que su ganancia viene");
    println!("  de mirar donde DEFLATE no puede, los tokens no le sirven, y D13 se cierra.");

    // ── EL TRATO, EN EUROS ──────────────────────────────────────────────────────────────────
    //
    // D13 en su forma ingenua COMPRA tiempo y PAGA tamaño. Quien decide es el dinero, y los dos
    // sumandos no se parecen: la CPU se paga UNA VEZ y el disco se alquila TODOS LOS MESES. Por
    // eso un ahorro de CPU tiene que ser enorme para compensar un byte de mas.
    //
    // Se compara: comprimir con la ventana de DEFLATE (rapido y gordo) contra la ventana entera
    // de zstd (lento y fino), sobre este mismo plano. Nada prestado de otro corpus.
    let medir = |wl: Option<u32>| -> (usize, f64) {
        let mut c = zstd::bulk::Compressor::new(nivel).unwrap();
        if let Some(w) = wl { let _ = c.set_parameter(zstd::zstd_safe::CParameter::WindowLog(w)); }
        let t = std::time::Instant::now();
        let z = c.compress(&d).map(|v| v.len()).unwrap_or(d.len());
        (z, t.elapsed().as_secs_f64())
    };
    let (b_def, _t_def) = medir(Some(15));
    let (b_zst, t_zst) = medir(None);
    // El tiempo del camino de D13 NO es el de comprimir con ventana de 32 KB: es el de comprimir
    // SIN buscar, porque las coincidencias vendrian dadas. La cota de eso es lo que tarda la
    // estrategia `fast`, que es lo unico que hemos medido que casi no busca.
    let t_d13 = {
        let mut c = zstd::bulk::Compressor::new(nivel).unwrap();
        use zstd::zstd_safe::zstd_sys::ZSTD_strategy::ZSTD_fast;
        let _ = c.set_parameter(zstd::zstd_safe::CParameter::Strategy(ZSTD_fast));
        let t = std::time::Instant::now();
        let _ = c.compress(&d);
        t.elapsed().as_secs_f64()
    };
    const EUR_H: f64 = 46.0 / (30.0 * 24.0);
    const PB: f64 = 1e15;
    const MESES: f64 = 12.0;
    let esc = PB / d.len() as f64;
    println!();
    println!("  ================= EL TRATO, EN EUROS POR PB DE PLANO =================");
    println!();
    println!("  {:<34} {:>14} {:>12}", "", "TB guardados", "horas de CPU");
    println!("  {:<34} {:>14.1} {:>12.1}", "zstd busca (lo de HOY)",
             b_zst as f64 * esc / 1e12, t_zst * esc / 3600.0);
    println!("  {:<34} {:>14.1} {:>12.1}", "D13: coincidencias dadas",
             b_def as f64 * esc / 1e12, t_d13 * esc / 3600.0);
    println!();
    for (nom, precio) in [("S3", 21.30), ("Backblaze", 5.55), ("bulk", 2.50), ("disco", 1.20)] {
        let eur = |b: usize, t: f64| (b as f64 * esc / 1e12) * precio * MESES
                                   + (t * esc / 3600.0) * EUR_H;
        let hoy = eur(b_zst, t_zst);
        let d13 = eur(b_def, t_d13);
        println!("  a precio {:<11} hoy {:>12.0} €/PB-año   D13 {:>12.0}   {}",
                 nom, hoy, d13,
                 if d13 < hoy { format!("GANA por {:.0}", hoy - d13) }
                 else { format!("PIERDE por {:.0}", d13 - hoy) });
    }
    println!();
    println!("  Y esto es la forma INGENUA de D13: tirar la busqueda entera. La que el propio");
    println!("  reparto de ventanas sugiere es otra —usar los tokens del DEFLATE como PUNTO DE");
    println!("  PARTIDA y buscar solo lo que queda lejos—, y esa no esta medida aqui.");
}
