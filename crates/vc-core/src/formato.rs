// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! Serialización del índice. Propia, en vez de serde+bincode.
//!
//! Dos razones más allá de quitar dependencias: (1) el formato en disco de un archivador
//! tiene que durar años y aquí queda escrito byte a byte en vez de delegado al detalle de
//! implementación de una librería; (2) con varints y deltas el índice ocupa bastante menos
//! que con los enteros de ancho fijo de bincode.
//!
//! El índice viene de un fichero que puede estar corrupto o ser hostil: TODA lectura va
//! comprobada, y ninguna longitud reserva memoria sin contrastarla antes con lo que queda
//! por leer.

use crate::{ChunkRec, FileRec, FrameRec, Hueco, Interior, Index, Transform};
use anyhow::{bail, Result};

// ---------- escritura ----------

#[derive(Default)]
struct Esc { v: Vec<u8> }

impl Esc {
    fn byte(&mut self, x: u8) { self.v.push(x); }
    /// LEB128 sin signo.
    fn var(&mut self, mut x: u64) {
        while x >= 0x80 { self.v.push((x as u8) | 0x80); x >>= 7; }
        self.v.push(x as u8);
    }
    /// Zigzag + LEB128: los negativos pequeños ocupan poco.
    fn zig(&mut self, x: i64) { self.var(((x << 1) ^ (x >> 63)) as u64); }
    fn bytes(&mut self, b: &[u8]) { self.var(b.len() as u64); self.v.extend_from_slice(b); }
    fn texto(&mut self, s: &str) { self.bytes(s.as_bytes()); }
}

/// Cuanto aporta cada seccion del indice, en CRUDO. Ver la nota de F2.66: el indice se
/// comprime de una sola pasada, asi que estas cifras son lo que cada parte mete en el
/// compresor, no bytes de archivo.
#[derive(Default)]
struct Desglose {
    rutas: u64, listas_chunk: u64, huellas: u64, modo_fecha: u64,
    esqueletos: u64, correcciones: u64, tramos_hueco: u64, resto_hueco: u64,
    referencias: u64, n_referencias: u64,
    recs_chunk: u64, recs_frame: u64,
}

pub fn escribir(idx: &Index) -> Vec<u8> {
    // Se reserva de golpe en vez de dejar que el Vec crezca doblando. Para un indice de 97 MB
    // el crecimiento por doblado son ~24 realojos y ~194 MB de memcpy acumulado, y el tamaño se
    // puede estimar antes de empezar porque ya se sabe cuantos ficheros y cuantos trozos hay.
    // La estimacion no tiene que acertar: si se queda corta el Vec crece como siempre, y si se
    // pasa, se devuelve un Vec con holgura que el que llama consume igual.
    let estimacion = idx.files.len() * 80 + idx.chunks.len() * 24 + 4096;
    let mut e = Esc { v: Vec::with_capacity(estimacion) };
    let mut dg = Desglose::default();
    let depurar = std::env::var("VC_INDICE_DEBUG").is_ok();
    // Huella de la carga util de cada transformada ya escrita -> numero del fichero que la
    // trae. Lo que se repite entre instantaneas no son unos pocos bytes: en pdfoffice son
    // 8,66 MB por copia. Aqui se escriben una vez.
    let mut vistas: std::collections::HashMap<[u8; 32], u64> = std::collections::HashMap::new();

    e.var(idx.files.len() as u64);
    for (fi, f) in idx.files.iter().enumerate() {
        let m0 = e.v.len();
        e.texto(&f.path);
        e.var(f.size);
        dg.rutas += (e.v.len() - m0) as u64;
        let m0 = e.v.len();
        // Los ids de chunk de un fichero suelen ir seguidos; en delta ocupan 1 byte.
        // Los saltos hacia atrás (aciertos de dedup) los cubre el zigzag.
        e.var(f.chunks.len() as u64);
        let mut prev: i64 = 0;
        for &c in &f.chunks {
            e.zig(c as i64 - prev);
            prev = c as i64;
        }
        dg.listas_chunk += (e.v.len() - m0) as u64;
        let m0 = e.v.len();
        e.v.extend_from_slice(&f.hash);
        dg.huellas += (e.v.len() - m0) as u64;
        // La carga se serializa aparte para poder mirarla antes de decidir si vale la pena
        // escribirla otra vez. Las transformadas sin carga (JXL, E8E9, LogTs) son un byte y
        // no pasan por aqui: buscarles un duplicado costaria mas que escribirlas.
        let mut sub = Esc::default();
        match &f.transform {
            None => sub.byte(0),
            Some(Transform::Deflate { esqueleto, huecos }) => {
                let e = &mut sub;
                dg.esqueletos += esqueleto.len() as u64;
                for h in huecos {
                    dg.correcciones += h.correcciones.len() as u64;
                    dg.tramos_hueco += (h.trozos.len() * 2) as u64;
                    if let Some(it) = &h.interior {
                        dg.esqueletos += it.esqueleto.len() as u64;
                        for h2 in &it.huecos {
                            dg.correcciones += h2.correcciones.len() as u64;
                            dg.tramos_hueco += (h2.trozos.len() * 2) as u64;
                        }
                    }
                }
                // La etiqueta 1 se reserva para lo que SIEMPRE ha significado: huecos sin
                // interior. Mientras no haya recursion el archivo sale byte a byte igual que
                // antes de que esto existiera, y cualquier binario anterior lo abre.
                // La 5 solo aparece cuando de verdad hay algo anidado, y un binario viejo
                // dira "transformada desconocida (5)", que es el mensaje correcto.
                // La etiqueta 5 hace falta si CUALQUIER campo nuevo lleva algo que decir:
                // un interior, o una clase que no sea la de siempre. Mirar solo el interior
                // funcionaba de casualidad mientras el unico caso nuevo -PNG almacenado-
                // traia interior por definicion; en cuanto aparecio uno con clase y SIN
                // interior -un JPEG a JXL- la clase se perdia al escribir, volvia como
                // "deflate" al leer, y el descompresor le aplicaba preflate a un flujo JXL.
                let rec = huecos.iter().any(|h| h.interior.is_some() || h.clase != crate::HUECO_DEFLATE);
                e.byte(if rec { 5 } else { 1 });
                e.bytes(esqueleto);
                e.var(huecos.len() as u64);
                for h in huecos { escribe_hueco(e, h, rec); }
            }
            Some(Transform::JpegJxl) => sub.byte(2),
            Some(Transform::E8e9) => sub.byte(3),
            Some(Transform::LogTs) => sub.byte(4),
        }
        // El umbral existe porque una referencia ocupa unos 3 bytes: por debajo de eso,
        // deduplicar cuesta mas de lo que ahorra y ademas ensucia el formato sin motivo.
        if sub.v.len() > 16 {
            let h = *blake3::hash(&sub.v).as_bytes();
            match vistas.get(&h) {
                Some(&j) => {
                    let m = e.v.len();
                    e.byte(7); e.var(j);
                    dg.referencias += (e.v.len() - m) as u64;
                    dg.n_referencias += 1;
                    // Lo que la referencia AHORRA es el tamaño de la carga que no se escribio.
                    dg.resto_hueco += sub.v.len() as u64;
                }
                None => { vistas.insert(h, fi as u64); e.v.extend_from_slice(&sub.v); }
            }
        } else {
            e.v.extend_from_slice(&sub.v);
        }
        e.var(f.mode as u64);
        e.zig(f.mtime);
        match &f.link_target {
            None => e.byte(0),
            Some(t) => { e.byte(1); e.texto(t); }
        }
        // VCMP0014: dueno, grupo, dispositivo y enlace duro. Van al final del registro para
        // que la parte de arriba siga siendo byte a byte la de antes.
        e.var(f.uid as u64);
        e.var(f.gid as u64);
        e.var(f.rdev);
        match f.enlace_a {
            None => e.byte(0),
            Some(j) => { e.byte(1); e.var(j as u64); }
        }
        // VCMP0016: atributos extendidos, al FINAL del registro para que todo lo de arriba siga
        // siendo byte a byte lo de antes.
        e.var(f.xattr.len() as u64);
        for (n, v) in &f.xattr { e.texto(n); e.bytes(v); }
    }

    let m0 = e.v.len();
    e.var(idx.chunks.len() as u64);
    let mut prev: i64 = 0;
    for c in &idx.chunks {
        e.zig(c.frame as i64 - prev); // no decreciente salvo el centinela u32::MAX
        prev = c.frame as i64;
        e.var(c.off as u64);
        e.var(c.len as u64);
        e.v.extend_from_slice(&c.hash);
    }

    dg.recs_chunk = (e.v.len() - m0) as u64;
    let m0 = e.v.len();
    e.var(idx.frames.len() as u64);
    let mut prev: u64 = 0;
    for f in &idx.frames {
        e.var(f.offset - prev); // estrictamente creciente
        prev = f.offset;
        e.var(f.csize);
        e.var(f.rsize);
        e.byte(f.codec);
        e.byte(f.lane);
    }

    dg.recs_frame = (e.v.len() - m0) as u64;
    e.zig(idx.nivel as i64);

    if depurar {
        let t = e.v.len() as f64;
        let pct = |x: u64| 100.0 * x as f64 / t;
        eprintln!("[indice] {} ficheros, {} chunks, {} frames — {} B en CRUDO",
                  idx.files.len(), idx.chunks.len(), idx.frames.len(), e.v.len());
        eprintln!("[indice] (crudo: lo que cada parte METE en el compresor. El indice se");
        eprintln!("[indice]  comprime de una pasada, asi que nadie tiene un tamaño propio.)");
        for (n, v) in [
            ("rutas", dg.rutas), ("listas de chunk", dg.listas_chunk),
            ("huellas de fichero", dg.huellas), ("modo+fecha+symlink", dg.modo_fecha),
            ("ESQUELETOS", dg.esqueletos), ("CORRECCIONES preflate", dg.correcciones),
            ("tramos de hueco", dg.tramos_hueco),
            ("registros de chunk", dg.recs_chunk), ("registros de frame", dg.recs_frame),
        ] {
            if v > 0 { eprintln!("[indice]   {:<24} {:>12} B  {:>5.1}%", n, v, pct(v)); }
        }
        if dg.n_referencias > 0 {
            eprintln!("[indice]   {:<24} {:>12} B   ({} referencias que ahorraron {} B)",
                      "referencias (F2.65)", dg.referencias, dg.n_referencias, dg.resto_hueco);
        }
    }
    e.v
}

// ---------- lectura ----------

struct Lec<'a> { d: &'a [u8], p: usize }

impl<'a> Lec<'a> {
    fn queda(&self) -> usize { self.d.len() - self.p }
    fn byte(&mut self) -> Result<u8> {
        if self.p >= self.d.len() { bail!("índice truncado"); }
        let b = self.d[self.p];
        self.p += 1;
        Ok(b)
    }
    fn var(&mut self) -> Result<u64> {
        let (mut x, mut s) = (0u64, 0u32);
        loop {
            let b = self.byte()?;
            if s >= 64 { bail!("varint desbordado en el índice"); }
            x |= ((b & 0x7F) as u64) << s;
            if b & 0x80 == 0 { return Ok(x); }
            s += 7;
        }
    }
    fn zig(&mut self) -> Result<i64> {
        let u = self.var()?;
        Ok(((u >> 1) as i64) ^ -((u & 1) as i64))
    }
    /// Longitud de una secuencia. Se contrasta con lo que queda por leer ANTES de reservar:
    /// cada elemento ocupa al menos un byte, así que una cuenta mayor es índice corrupto.
    fn cuenta(&mut self) -> Result<usize> {
        let n = self.var()? as usize;
        if n > self.queda() { bail!("índice corrupto: cuenta {n} mayor que los bytes restantes"); }
        Ok(n)
    }
    fn bytes(&mut self) -> Result<&'a [u8]> {
        let n = self.var()? as usize;
        if n > self.queda() { bail!("índice corrupto: longitud {n} mayor que los bytes restantes"); }
        let s = &self.d[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn texto(&mut self) -> Result<String> {
        Ok(String::from_utf8(self.bytes()?.to_vec())?)
    }
    fn hash(&mut self) -> Result<[u8; 16]> {
        if self.queda() < 16 { bail!("índice truncado en un hash"); }
        let mut h = [0u8; 16];
        h.copy_from_slice(&self.d[self.p..self.p + 16]);
        self.p += 16;
        Ok(h)
    }
}

/// `con_huellas`: si el contenedor es VCMP0013 o posterior, cada trozo trae su huella de 16
/// bytes. Se pasa explicito desde quien abrio el archivo y vio el MAGIC, en vez de adivinarlo
/// contando bytes restantes: adivinar el formato es como se corrompen los indices.
pub fn leer(datos: &[u8], con_huellas: bool) -> Result<Index> {
    leer_pleno(datos, con_huellas, false)
}

/// `con_meta`: VCMP0014 o posterior guarda dueno, grupo, dispositivo y enlaces duros. Los
/// anteriores no los traen y se leen con los valores por defecto, que es exactamente lo que
/// hacian antes de que existieran: al extraer, los ficheros quedan del usuario que restaura.
pub fn leer_pleno(datos: &[u8], con_huellas: bool, con_meta: bool) -> Result<Index> {
    leer_v(datos, con_huellas, con_meta, false)
}

/// `con_xattr`: VCMP0016 o posterior.
pub fn leer_v(datos: &[u8], con_huellas: bool, con_meta: bool, con_xattr: bool) -> Result<Index> {
    let mut l = Lec { d: datos, p: 0 };
    let mut idx = Index::default();

    let nf = l.cuenta()?;
    idx.files.reserve(nf);
    for _ in 0..nf {
        let path = l.texto()?;
        let size = l.var()?;
        let nc = l.cuenta()?;
        let mut chunks = Vec::with_capacity(nc);
        let mut prev: i64 = 0;
        for _ in 0..nc {
            prev += l.zig()?;
            if prev < 0 { bail!("índice corrupto: id de chunk negativo"); }
            chunks.push(prev as u64);
        }
        let hash = l.hash()?;
        let transform = match l.byte()? {
            0 => None,
            // 1 y 5 son la MISMA transformada; la 5 solo anade el campo de interior en
            // cada hueco. Comparten camino para que no puedan divergir con el tiempo.
            tag @ (1 | 5) => {
                let esqueleto = l.bytes()?.to_vec();
                let nh = l.cuenta()?;
                let mut huecos = Vec::with_capacity(nh);
                for _ in 0..nh { huecos.push(lee_hueco(&mut l, tag == 5)?); }
                Some(Transform::Deflate { esqueleto, huecos })
            }
            2 => Some(Transform::JpegJxl),
            3 => Some(Transform::E8e9),
            4 => Some(Transform::LogTs),
            // F2.65: la carga ya la trae un fichero anterior. Solo se referencia HACIA ATRAS,
            // asi que al llegar aqui ese fichero ya esta leido. Se comprueba igualmente en vez
            // de confiar: un indice manipulado no debe poder hacernos indexar fuera de rango
            // ni entrar en un ciclo.
            7 => {
                let j = l.var()? as usize;
                if j >= idx.files.len() {
                    bail!("índice corrupto: transformada compartida apunta al fichero {j}, \
                           que aún no existe");
                }
                idx.files[j].transform.clone()
            }
            t => bail!("transformada desconocida ({t}): ¿archivo de una versión posterior?"),
        };
        let mode = l.var()? as u32;
        let mtime = l.zig()?;
        let link_target = match l.byte()? {
            0 => None,
            1 => Some(l.texto()?),
            t => bail!("marca de symlink inválida ({t})"),
        };
        let (uid, gid, rdev, enlace_a) = if con_meta {
            let u = l.var()? as u32;
            let g = l.var()? as u32;
            let rd = l.var()?;
            let en = match l.byte()? {
                0 => None,
                1 => {
                    let j = l.var()? as u32;
                    if j as usize >= idx.files.len() {
                        bail!("índice corrupto: enlace duro al fichero {j}, que aún no existe");
                    }
                    Some(j)
                }
                t => bail!("marca de enlace duro inválida ({t})"),
            };
            (u, g, rd, en)
        } else { (0, 0, 0, None) };
        let xattr = if con_xattr {
            let n = l.cuenta()?;
            let mut v = Vec::with_capacity(n);
            for _ in 0..n { v.push((l.texto()?, l.bytes()?.to_vec())); }
            v
        } else { Vec::new() };
        idx.files.push(FileRec { path, size, chunks, hash, transform, mode, mtime, link_target, uid, gid, rdev, enlace_a, xattr });
    }

    let nc = l.cuenta()?;
    idx.chunks.reserve(nc);
    let mut prev: i64 = 0;
    for _ in 0..nc {
        prev += l.zig()?;
        if prev < 0 || prev > u32::MAX as i64 { bail!("índice corrupto: id de frame fuera de rango"); }
        let frame = prev as u32;
        let off = l.var()? as u32;
        let len = l.var()? as u32;
        // Un archivo VCMP0012 no lleva huellas: se lee con hash a cero y se puede abrir y
        // extraer igual. Lo unico que no admite es que le anadan una instantanea encima.
        let hash = if con_huellas { l.hash()? } else { [0u8; 16] };
        idx.chunks.push(ChunkRec { frame, off, len, hash });
    }

    let nfr = l.cuenta()?;
    idx.frames.reserve(nfr);
    let mut pos: u64 = 0;
    for _ in 0..nfr {
        pos = pos.checked_add(l.var()?).ok_or_else(|| anyhow::anyhow!("índice corrupto: offset desbordado"))?;
        let csize = l.var()?;
        let rsize = l.var()?;
        let codec = l.byte()?;
        let lane = l.byte()?;
        idx.frames.push(FrameRec { offset: pos, csize, rsize, codec, lane });
    }

    idx.nivel = l.zig()? as i32;

    // Coherencia: cada chunk tiene que apuntar a un frame que exista, y cada fichero a
    // chunks que existan. Se comprueba aquí, una vez, en vez de indexar a ciegas después.
    for c in &idx.chunks {
        if c.frame as usize >= idx.frames.len() {
            bail!("índice incoherente: chunk apunta al frame {} de {}", c.frame, idx.frames.len());
        }
        let fr = &idx.frames[c.frame as usize];
        if c.off as u64 + c.len as u64 > fr.rsize {
            bail!("índice incoherente: chunk se sale de su frame");
        }
    }
    for f in &idx.files {
        for &c in &f.chunks {
            if c as usize >= idx.chunks.len() {
                bail!("índice incoherente: {} apunta al chunk {} de {}", f.path, c, idx.chunks.len());
            }
        }
    }
    Ok(idx)
}


/// Un hueco. `rec` dice si el formato lleva el campo de interior; con la etiqueta 1 no existe
/// y no se escribe ni un byte de mas, que es lo que mantiene los archivos viejos idénticos.
fn escribe_hueco(e: &mut Esc, h: &Hueco, rec: bool) {
    e.var(h.trozos.len() as u64);
    let mut prev = 0u32;
    for &(o, l) in &h.trozos {
        e.var((o - prev) as u64); // los tramos van en orden
        prev = o + l;
        e.var(l as u64);
    }
    e.bytes(&h.correcciones);
    e.var(h.len_plano as u64);
    if rec {
        e.byte(h.clase);
        match &h.interior {
            None => e.byte(0),
            Some(it) => {
                e.byte(1);
                e.bytes(&it.esqueleto);
                e.var(it.huecos.len() as u64);
                // Los huecos interiores nunca llevan a su vez interior (la profundidad esta
                // acotada a uno), asi que se escriben en la forma corta.
                for h2 in &it.huecos { escribe_hueco(e, h2, false); }
            }
        }
    }
}

fn lee_hueco(l: &mut Lec, rec: bool) -> Result<Hueco> {
    // `cuenta` y los topes de rango no son adorno: el indice viene de un fichero que puede
    // estar corrupto o fabricado, y un `with_capacity` con un numero inventado es una
    // reserva de memoria arbitraria a peticion de quien escribio el archivo.
    let nt = l.cuenta()?;
    let mut trozos = Vec::with_capacity(nt);
    let mut prev = 0u64;
    for _ in 0..nt {
        let o = prev + l.var()?;
        let len = l.var()?;
        if o > u32::MAX as u64 || len > u32::MAX as u64 { bail!("tramo fuera de rango"); }
        prev = o + len;
        trozos.push((o as u32, len as u32));
    }
    let correcciones = l.bytes()?.to_vec();
    let lp = l.var()?;
    if lp > u32::MAX as u64 { bail!("plano fuera de rango"); }
    let len_plano = lp as u32;
    let clase = if rec {
        let c = l.byte()?;
        if c > crate::HUECO_JXL { bail!("clase de hueco desconocida ({c}): ¿archivo de una versión posterior?"); }
        c
    } else { crate::HUECO_DEFLATE };
    let interior = if rec && l.byte()? == 1 {
        let esqueleto = l.bytes()?.to_vec();
        let n = l.cuenta()?;
        let mut huecos = Vec::with_capacity(n);
        for _ in 0..n { huecos.push(lee_hueco(l, false)?); }
        Some(Box::new(Interior { esqueleto, huecos }))
    } else { None };
    Ok(Hueco { trozos, correcciones, len_plano, interior, clase })
}
