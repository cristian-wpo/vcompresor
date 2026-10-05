// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! ¿CUANTO DE LO QUE GUARDAMOS SE PUEDE ADIVINAR? La medida que decide si la fase 3 es posible.
//!
//! DE DONDE VIENE ESTA PREGUNTA. vdeflate funciona en las dos direcciones sobre los 1.973 flujos
//! del banco, pero NO puede entrar en el producto: guardamos el 104,51% del DEFLATE y preflate
//! guarda el 1,70%, o sea 61,5 veces menos. Y el desglose dice exactamente donde esta todo: el
//! 98,8% de nuestro coste es DESCRIBIR LAS COPIAS, 96.782.028 de ellas.
//!
//! LA IDEA DE LA FASE 3, que es la de preflate. Esas copias no son informacion arbitraria: son
//! lo que un buscador de coincidencias ENCONTRO al comprimir. Si nosotros corremos un buscador
//! parecido sobre el mismo plano, deberia encontrar las mismas. Y entonces no hay que guardar la
//! copia: hay que guardar SOLO DONDE NOS EQUIVOCAMOS.
//!
//! ESTE PROGRAMA NO CONSTRUYE ESO. Mide si se sostiene, que es lo que hay que saber antes de
//! gastar dias en ello. Corre el buscador de referencia -cadena de hash, coincidencia mas larga
//! y entre iguales la mas cercana, que es lo que hace zlib- y compara token a token contra lo
//! que el codificador original decidio de verdad.
//!
//! LAS CUATRO RESPUESTAS, y cada una cuesta un dinero distinto:
//!
//!   EXACTA          misma longitud y misma distancia. No cuesta NADA guardarla: se adivina.
//!   OTRA DISTANCIA  misma longitud, otra distancia. Cuesta el RANGO de la que se eligio dentro
//!                   de las candidatas de esa longitud, ordenadas de la mas cercana. Si el rango
//!                   suele ser 0, 1 o 2, son tres bits y no dieciseis.
//!   MAS CORTA       el original eligio una coincidencia mas corta que la mejor. Eso es
//!                   emparejamiento PEREZOSO: el codificador prefirio esperar al byte siguiente.
//!                   Se puede modelar; hay que saber cuanto pasa.
//!   MAS LARGA       nosotros encontramos menos de lo que habia. Solo puede pasar por el tope
//!                   de la cadena, asi que es una cota de ESTE programa y no del formato — y por
//!                   eso el tope se imprime al lado del resultado en vez de esconderse.
//!
//! Lo que salga es EL TECHO de la fase 3: si el 90% son exactas, la fase 3 funciona y el hueco
//! de 61,5x se cierra. Si son el 40%, hay que pensar otra cosa antes de escribir nada.
//!
//!   uso:  vc-prediccion [directorio-del-banco] [max-flujos]

use std::path::PathBuf;
use vc_core::inflate::{descomprime_con_bloques, Bloque, Token};

const VENTANA: usize = 32 * 1024;
const MASCARA: usize = VENTANA - 1;
const CUBOS: usize = 1 << 15;
const MIN_COPIA: usize = 3;
const MAX_COPIA: usize = 258;
/// Tope de pasos por cadena. NO es del formato, es de este programa: con la cadena entera esto
/// tardaria horas sobre 6 GB de plano. Se imprime al lado del resultado porque es justo lo que
/// convierte un «mas larga» en un artefacto nuestro en vez de un hallazgo.
const CADENA_MAX: usize = 128;

#[inline(always)]
fn h3(d: &[u8], p: usize) -> usize {
    (((d[p] as usize) << 10) ^ ((d[p + 1] as usize) << 5) ^ (d[p + 2] as usize)) & (CUBOS - 1)
}

/// Cuanto coinciden dos trozos del mismo buffer, hasta `tope`.
#[inline(always)]
fn largo_comun(d: &[u8], a: usize, b: usize, tope: usize) -> usize {
    let mut k = 0;
    while k < tope && d[a + k] == d[b + k] { k += 1 }
    k
}

fn main() {
    let dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/deflate-banco".into());
    let tope_flujos: usize = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(usize::MAX);
    let mut casos: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(it) => it.filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "raw").unwrap_or(false)).collect(),
        Err(e) => { eprintln!("no puedo leer {dir}: {e}"); std::process::exit(2); }
    };
    casos.sort();
    casos.truncate(tope_flujos);

    let (mut n_flujos, mut n_copias) = (0u64, 0u64);
    let (mut exacta, mut otra_dist, mut mas_corta, mut mas_larga, mut sin_nada) = (0u64, 0u64, 0u64, 0u64, 0u64);
    // Rango de la distancia elegida entre las candidatas de la MISMA longitud, de la mas cercana
    // a la mas lejana. 0 = el original eligio justo la que habriamos elegido nosotros.
    let mut rango = [0u64; 9];          // 0..=7 y «8 o mas» en la ultima
    let mut recorte_long = [0u64; 8];   // cuanto mas corta se quedo, 1..=7 y «8 o mas»
    let mut deflate_total = 0usize;

    // Reutilizadas entre flujos para no pedir 160 KB por cada uno de los 1.973.
    let mut head = vec![u32::MAX; CUBOS];
    let mut prev = vec![u32::MAX; VENTANA];
    let mut cl = [0u16; CADENA_MAX];
    let mut cd = [0u16; CADENA_MAX];

    for p in &casos {
        let Ok(d) = std::fs::read(p) else { continue };
        let Ok((plano, bloques)) = descomprime_con_bloques(&d) else { continue };
        if plano.len() < MIN_COPIA { continue }
        deflate_total += d.len();
        n_flujos += 1;
        head.iter_mut().for_each(|x| *x = u32::MAX);
        prev.iter_mut().for_each(|x| *x = u32::MAX);
        let n = plano.len();
        let mut pos = 0usize;

        // Mete la posicion `p` en la cadena de hash. Solo si quedan 3 bytes: por debajo de eso
        // no hay coincidencia posible en DEFLATE.
        macro_rules! mete {
            ($q:expr) => {{
                let q = $q;
                if q + MIN_COPIA <= n {
                    let hh = h3(&plano, q);
                    prev[q & MASCARA] = head[hh];
                    head[hh] = q as u32;
                }
            }};
        }

        for b in &bloques {
            match b {
                Bloque::Almacenado { datos, .. } => {
                    for k in 0..datos.len() { mete!(pos + k); }
                    pos += datos.len();
                }
                Bloque::Comprimido { tokens, .. } => {
                    for t in tokens {
                        match t {
                            Token::Lit(_) => { mete!(pos); pos += 1; }
                            Token::Copia { long, dist, .. } => {
                                let (long, dist) = (*long as usize, *dist as usize);
                                n_copias += 1;
                                // ¿Que habria encontrado el buscador de referencia AQUI?
                                let tope = MAX_COPIA.min(n - pos);
                                let mut nc = 0usize;
                                if pos + MIN_COPIA <= n && tope >= MIN_COPIA {
                                    let hh = h3(&plano, pos);
                                    let mut c = head[hh];
                                    let mut pasos = 0;
                                    while c != u32::MAX && pasos < CADENA_MAX {
                                        let cp = c as usize;
                                        let dd = pos - cp;
                                        if dd == 0 || dd > VENTANA { break }
                                        let l = largo_comun(&plano, cp, pos, tope);
                                        if l >= MIN_COPIA && nc < CADENA_MAX {
                                            cl[nc] = l as u16; cd[nc] = dd as u16; nc += 1;
                                        }
                                        c = prev[cp & MASCARA];
                                        pasos += 1;
                                    }
                                }
                                if nc == 0 {
                                    sin_nada += 1;
                                } else {
                                    let mejor = *cl[..nc].iter().max().unwrap() as usize;
                                    if mejor > long {
                                        // El original se quedo corto a proposito: emparejamiento
                                        // perezoso. Cuanto se dejo es lo que hay que modelar.
                                        mas_corta += 1;
                                        let r = (mejor - long).min(8);
                                        recorte_long[r.saturating_sub(1)] += 1;
                                    } else if mejor < long {
                                        mas_larga += 1;   // artefacto del tope de cadena
                                    } else {
                                        // Misma longitud. ¿La misma distancia? Y si no, ¿que
                                        // rango ocupa la suya entre las de esa longitud?
                                        let mut r = 0usize; let mut visto = false;
                                        for k in 0..nc {
                                            if cl[k] as usize != mejor { continue }
                                            if cd[k] as usize == dist { visto = true; break }
                                            r += 1;
                                        }
                                        if visto && r == 0 { exacta += 1; }
                                        else if visto { otra_dist += 1; rango[r.min(8)] += 1; }
                                        else { otra_dist += 1; rango[8] += 1; }
                                    }
                                }
                                for k in 0..long { mete!(pos + k); }
                                pos += long;
                            }
                        }
                    }
                }
            }
        }
    }

    let pc = |x: u64| if n_copias == 0 { 0.0 } else { 100.0 * x as f64 / n_copias as f64 };
    println!();
    println!("  ¿SE PUEDEN ADIVINAR LAS COPIAS? — {n_flujos} flujos, {n_copias} copias");
    println!("  buscador de referencia: coincidencia mas larga, entre iguales la mas cercana.");
    println!("  tope de cadena {CADENA_MAX} pasos (es una cota DE ESTE PROGRAMA, no del formato)");
    println!();
    println!("  {:<34} {:>13}  {:>7}", "EXACTA (long y dist)", exacta, format!("{:.2}%", pc(exacta)));
    println!("  {:<34} {:>13}  {:>7}", "misma long, otra dist", otra_dist, format!("{:.2}%", pc(otra_dist)));
    println!("  {:<34} {:>13}  {:>7}", "el original se quedo mas corto", mas_corta, format!("{:.2}%", pc(mas_corta)));
    println!("  {:<34} {:>13}  {:>7}", "nosotros encontramos menos", mas_larga, format!("{:.2}%", pc(mas_larga)));
    println!("  {:<34} {:>13}  {:>7}", "no encontramos nada", sin_nada, format!("{:.2}%", pc(sin_nada)));
    println!();
    println!("  DE LAS DE «otra dist», que rango ocupaba la que se eligio:");
    let tot_r: u64 = rango.iter().sum();
    for (i, v) in rango.iter().enumerate() {
        if *v == 0 { continue }
        let et = if i == 8 { "8 o mas / fuera de la cadena".to_string() } else { format!("rango {i}") };
        println!("    {:<30} {:>13}  {:>7}", et, v,
                 format!("{:.1}%", if tot_r == 0 { 0.0 } else { 100.0 * *v as f64 / tot_r as f64 }));
    }
    println!();
    println!("  DE LAS «mas cortas», cuanto se dejo sin coger:");
    let tot_c: u64 = recorte_long.iter().sum();
    for (i, v) in recorte_long.iter().enumerate() {
        if *v == 0 { continue }
        let et = if i == 7 { "8 bytes o mas".to_string() } else { format!("{} byte(s)", i + 1) };
        println!("    {:<30} {:>13}  {:>7}", et, v,
                 format!("{:.1}%", if tot_c == 0 { 0.0 } else { 100.0 * *v as f64 / tot_c as f64 }));
    }
    println!();
    // LO QUE ESTO CUESTA EN BYTES, que es la unica forma de compararlo con preflate.
    //
    // Y AQUI ESTA LA PARTE QUE SE OLVIDA: no basta con guardar los fallos, hay que DECIR CUALES
    // SON. Un bit por copia diciendo «esta se adivino» son 96.782.028 bits = 12,1 MB = el 3,23%
    // del DEFLATE, casi el DOBLE de todo lo que guarda preflate. O sea que la marca cruda ya
    // pierde la partida ella sola, antes de guardar una sola correccion.
    //
    // La marca hay que CODIFICARLA POR ENTROPIA. Una senial que sale que si el 97% de las veces
    // no cuesta un bit: cuesta H(p) bits, y con p = 2,9% eso son 0,19 bits. Es la diferencia
    // entre 12,1 MB y 2,3 MB, y es lo que decide si la fase 3 es viable.
    let fallos = otra_dist + mas_corta + mas_larga + sin_nada;
    let p = if n_copias == 0 { 0.0 } else { fallos as f64 / n_copias as f64 };
    let h = if p <= 0.0 || p >= 1.0 { 0.0 }
            else { -p * p.log2() - (1.0 - p) * (1.0 - p).log2() };
    let bits_marca = n_copias as f64 * h;
    let bits_marca_cruda = n_copias as f64;
    // Y las correcciones de los que fallan. Cotas optimistas y dichas: el rango casi siempre
    // cabe en 3 bits, el recorte perezoso en 6, y una copia que no se adivina cuesta entera.
    let bits_corr = otra_dist as f64 * 3.0 + mas_corta as f64 * 6.0
                  + (mas_larga + sin_nada) as f64 * 24.0;
    let by = |b: f64| (b / 8.0) as usize;
    let pc_d = |b: f64| 100.0 * (b / 8.0) / deflate_total.max(1) as f64;
    println!("  LO QUE COSTARIA GUARDAR SOLO LOS FALLOS — DEFLATE de entrada {deflate_total} B");
    println!();
    println!("    {:<40} {:>12} B  {:>7.3}%", "marca «acerte» CRUDA, 1 bit por copia",
             by(bits_marca_cruda), pc_d(bits_marca_cruda));
    println!("    {:<40} {:>12} B  {:>7.3}%",
             format!("la misma por entropia, H({:.3}%)={:.3} bits", 100.0 * p, h),
             by(bits_marca), pc_d(bits_marca));
    println!("    {:<40} {:>12} B  {:>7.3}%", "las correcciones de los que fallan",
             by(bits_corr), pc_d(bits_corr));
    println!("    {:<40} {:>12} B  {:>7.3}%", "TOTAL con la marca por entropia",
             by(bits_marca + bits_corr), pc_d(bits_marca + bits_corr));
    println!();
    println!("    {:<40} {:>12} B  {:>7.3}%", "preflate, medido sobre este mismo banco", 6371168,
             100.0 * 6371168.0 / deflate_total.max(1) as f64);
    println!("    {:<40} {:>12} {:>10}", "nosotros HOY, describiendolo todo", "—", "104,51%");
    println!();
    println!("  COMO LEER ESTO. La marca cruda pierde ella sola: 1 bit por copia ya es el doble");
    println!("  de todo preflate. Codificada por entropia cuesta {:.0} veces menos, y ahi es",
             if h > 0.0 { 1.0 / h } else { 0.0 });
    println!("  donde se decide la fase 3. Las cotas de correccion -3, 6 y 24 bits- son");
    println!("  OPTIMISTAS a proposito: si ni asi se baja del 1,70%, no se gana con ESTE");
    println!("  buscador y hay que estimar parametros por flujo, que es lo que hace preflate.");
}
