// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! EL REPLAY: probar configuraciones del mezclador sin volver a correr los modelos.
//!
//! POR QUE EXISTE. Medir una configuracion cuesta hoy una compresion entera —3,65 s por 512 KiB
//! en el Mac, 82 s de mediana en el banco del servidor—, y de esos segundos casi todo se va en
//! los modelos: buscar en las tablas de contexto, avanzar los estados, la cadena ISSE. El
//! mezclador y la calibracion son la cola, y son baratos. Pero cada vez que se quiere probar otro
//! `VC_MIX_LR` se vuelve a pagar todo el modelo para llegar a la cola.
//!
//! Aqui se paga UNA VEZ. `VC_VOLCAR` guarda lo que los modelos predijeron bit a bit, y esto
//! ejecuta la cola encima, tantas veces como haga falta.
//!
//! Y NO ES UNA APROXIMACION, que es lo que separa esto de un atajo: los submodelos aprenden del
//! BIT, no de la prediccion final, asi que su trayectoria no depende de como este configurada la
//! cola. Volcar `st[]` no pierde nada. Ademas el replay no re-implementa la mezcla: llama a
//! `mezcla()` y `actualiza_mezcla()`, los mismos metodos que usa `comprimir`. Divergir tendria
//! que ser un acto deliberado.
//!
//!   vc-cm c muestra.bin /dev/null           con VC_VOLCAR=/tmp/v.st
//!   vc-replay /tmp/v.st                      la config por defecto (control)
//!   vc-replay /tmp/v.st --barrido --jobs 8   la rejilla de la cola
use std::sync::atomic::{AtomicUsize, Ordering};
use vc_cm::{lee_volcado, replay, replay_muchas, CfgMezcla};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() < 2 {
        eprintln!("uso: vc-replay <volcado> [--barrido] [--jobs N]");
        std::process::exit(2);
    }
    let barrido = a.iter().any(|x| x == "--barrido");
    let jobs = a.iter().position(|x| x == "--jobs")
        .and_then(|i| a.get(i + 1)).and_then(|x| x.parse().ok())
        .unwrap_or(std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));

    let v = match lee_volcado(&a[1]) {
        Ok(v) => v,
        Err(e) => { eprintln!("{e}"); std::process::exit(3); }
    };
    eprintln!("volcado: {} bits · {} B de dato · familia {} modelo {} perfil {} · registro {} B",
              v.bits, v.tam, v.familia, v.modelo, v.perfil, v.reg);

    if !barrido && !a.iter().any(|x| x == "--sse-w0") {
        let t0 = std::time::Instant::now();
        let (bytes, ent) = replay(&v, &CfgMezcla::default());
        println!("bytes={bytes} entropia={:.0}B perdida_codificador={:+.4}% en {:.2}s",
                 ent / 8.0, 100.0 * (bytes as f64 - ent / 8.0) / (ent / 8.0),
                 t0.elapsed().as_secs_f64());
        return;
    }

    // ── el barrido FINO del peso inicial de la calibracion ─────────────────────────────────────
    //
    // POR QUE ESTE Y NO OTRO. `sse_w` arranca en 49152 —«49152/65536 = 3/4, la mezcla 3:1 de
    // siempre»— y ese 3/4 viene de la version anterior, cuando la mezcla era FIJA. Al hacerse
    // aprendida, el 3/4 dejo de ser la mezcla y paso a ser solo el PUNTO DE PARTIDA, y nadie
    // volvio a preguntarse si es el bueno. Es una constante «A OJO» de manual.
    //
    // Y es el ejemplo de lo que el volcado desbloquea: es un espacio CONTINUO (0..65536). A lo
    // bruto, probar 33 puntos son 33 compresiones y por eso no se ha hecho nunca; aqui son 33
    // replays del mismo volcado. La rejilla de valores discretos no puede con esto, no por falta
    // de tiempo sino porque nadie plantea barrer un continuo cuando cada punto cuesta minutos.
    if a.iter().any(|x| x == "--sse-w0") {
        let pasos: Vec<i32> = (0..=32).map(|i| i * 2048).collect();
        let combos: Vec<CfgMezcla> = pasos.iter()
            .map(|&w| CfgMezcla { sse_w0: Some(w), ..Default::default() }).collect();
        eprintln!("barrido fino de sse_w0: {} puntos de 0 a 65536", combos.len());
        let t0 = std::time::Instant::now();
        let r = replay_muchas(&v, &combos);
        println!("sse_w0\tfraccion\tbytes");
        for (c, (b, _)) in combos.iter().zip(&r) {
            let w = c.sse_w0.unwrap();
            println!("{w}\t{:.3}\t{b}", w as f64 / 65536.0);
        }
        let (mi, (mb, _)) = r.iter().enumerate().min_by_key(|(_, (b, _))| *b).unwrap();
        let base = r[24].0;   // 24*2048 = 49152, el de hoy
        eprintln!("hoy: 49152 (3/4) -> {base} B · mejor: {} -> {mb} B ({:+.3}%)",
                  pasos[mi], 100.0 * (*mb as f64 - base as f64) / base as f64);
        if mi == 0 || mi == pasos.len() - 1 {
            eprintln!("⚠️ GANA EN EL BORDE: el barrido se quedo corto, hay que extenderlo.");
        }
        eprintln!("{} puntos en {:.1}s", r.len(), t0.elapsed().as_secs_f64());
        return;
    }

    // ── la rejilla de la cola ──────────────────────────────────────────────────────────────────
    // Los mismos valores que barre bench/mezclador.sh, para que las dos medidas se puedan
    // comparar fila a fila. VC_ISSE_LR no esta: no vive aguas abajo de `st[]` y por tanto un
    // volcado no puede contestar por el. Decirlo es parte del resultado.
    let mix_lr = [8u32, 10, 11, 12, 13, 14, 16];
    let decay = [0u32, 14, 16, 18];
    let n_sse = [3usize, 4, 5];
    let mut combos = Vec::new();
    for &m in &mix_lr { for &d in &decay { for &s in &n_sse {
        combos.push(CfgMezcla { mix_lr: Some(m), decay: Some(d), n_sse: Some(s), sse_w0: None });
    }}}
    eprintln!("barrido: {} configuraciones de la cola · {jobs} hilos", combos.len());
    eprintln!("⚠️ VC_ISSE_LR no entra: mueve st[IISSE], o sea que cambia el propio volcado.");

    // Se reparte POR LOTES, no configuracion a configuracion. `replay_muchas` reutiliza un solo
    // `Predictor` dentro del lote, y crear el predictor cuesta mas que ejecutar la cola: pedirlas
    // de una en una devolvia el coste que este binario existe para evitar.
    let t0 = std::time::Instant::now();
    let lote = combos.len().div_ceil(jobs.max(1));
    let siguiente = AtomicUsize::new(0);
    let res: Vec<(usize, usize)> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..jobs).map(|_| s.spawn(|| {
            let mut mios = Vec::new();
            loop {
                let i0 = siguiente.fetch_add(lote, Ordering::SeqCst);
                if i0 >= combos.len() { break; }
                let i1 = (i0 + lote).min(combos.len());
                for (j, (bytes, _)) in replay_muchas(&v, &combos[i0..i1]).into_iter().enumerate() {
                    mios.push((i0 + j, bytes));
                }
            }
            mios
        })).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    let mut res = res;
    res.sort_unstable();
    let seg = t0.elapsed().as_secs_f64();

    println!("# barrido de la cola sobre {} bits volcados", v.bits);
    println!("VC_MIX_LR\tVC_DECAY\tVC_SSE\tbytes");
    for (i, b) in &res {
        let c = &combos[*i];
        println!("{}\t{}\t{}\t{}", c.mix_lr.unwrap(), c.decay.unwrap(), c.n_sse.unwrap(), b);
    }
    let (mi, mb) = res.iter().min_by_key(|(_, b)| *b).unwrap();
    let c = &combos[*mi];
    eprintln!("mejor: VC_MIX_LR={} VC_DECAY={} VC_SSE={} -> {} bytes",
              c.mix_lr.unwrap(), c.decay.unwrap(), c.n_sse.unwrap(), mb);
    eprintln!("{} configuraciones en {:.1}s = {:.2}s cada una (⚠️ reloj de referencia, no medida)",
              res.len(), seg, seg / res.len() as f64);
}
