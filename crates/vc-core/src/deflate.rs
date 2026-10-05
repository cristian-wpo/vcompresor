// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! Localiza los flujos DEFLATE que viven DENTRO de otros formatos.
//!
//! La idea del carril: no competimos contra el dato, competimos contra un codificador de
//! 1993 metido en formatos de 2026. `.zip`, `.png`, `.docx`, `.xlsx`, `.pptx`, `.jar`,
//! `.apk`, `.epub` y `.gz` llevan todos DEFLATE dentro — **un solo carril los cubre todos**.
//!
//! Este módulo sólo dice DÓNDE están los flujos. Desplegarlos, verificarlos y volver a
//! crearlos es cosa de `lib.rs`, igual para todos.
//!
//! Un flujo puede venir troceado (los IDAT de un PNG), de ahí que cada hueco sea una lista
//! de tramos y no un solo rango.

/// Tramos `(offset, longitud)` del fichero original que forman UN flujo deflate crudo,
/// en orden. Concatenados son el flujo; al recrearlo se vuelve a partir por esas longitudes.
pub type Flujo = Vec<(u32, u32)>;

/// Una region de interes dentro del fichero: o es un flujo DEFLATE, o es un miembro
/// ALMACENADO (metodo 0). La distincion importa porque solo el primero se recrea con
/// preflate; el segundo son bytes que ya estaban tal cual.
#[derive(Clone)]
pub struct Region { pub tramos: Flujo, pub crudo: bool }

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum Formato { Gzip, Zip, Png, Jpeg, Pdf, Otro }

/// Detecta el formato por la FIRMA del contenido, no por la extensión. Un PNG llamado
/// `foto.jpg` tiene que ir al carril que le toca (pasa: 58 casos en el corpus real).
pub fn firma(d: &[u8]) -> Formato {
    if d.len() >= 8 && d[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] { return Formato::Png; }
    if d.len() >= 4 && d[0] == 0x1F && d[1] == 0x8B && d[2] == 8 { return Formato::Gzip; }
    if d.len() >= 4 && d[..2] == [b'P', b'K'] && (d[2] == 3 || d[2] == 5 || d[2] == 7) { return Formato::Zip; }
    if d.len() >= 3 && d[0] == 0xFF && d[1] == 0xD8 && d[2] == 0xFF { return Formato::Jpeg; }
    if d.len() >= 5 && d[..5] == *b"%PDF-" { return Formato::Pdf; }
    Formato::Otro
}

/// Los flujos deflate del fichero, según su formato. `None` = este carril no aplica.
pub fn flujos(d: &[u8]) -> Option<Vec<Flujo>> {
    match firma(d) {
        Formato::Gzip => gzip(d).map(|f| vec![f]),
        Formato::Png => png(d),
        Formato::Zip => zip(d),
        Formato::Pdf if pdf_encendido() => pdf(d),
        _ => None,
    }
}

/// `VC_SIN_PDF=1` apaga el carril de PDF. Existe para dos cosas: la ABLACION —sin perilla,
/// «cuanto compra el PDF» solo se contesta comparando dos binarios, y esa diferencia lleva
/// dentro todo lo demas que haya cambiado— y para que **`--price` pueda apagarlo**.
///
/// ⚠️ SE LEE EN CADA LLAMADA, Y NO ES DESCUIDO. La primera version la cacheaba en un `OnceLock`
/// y eso HABRIA ROTO LA SONDA en silencio: `sonda_carriles` corre la tuberia entera varias veces
/// poniendo y quitando la variable entre pasadas (`lib.rs`, el bloque de los carriles), asi que
/// un valor cacheado en la primera pasada habria decidido las tres. Es la misma forma de leerla
/// que `apagado()`, y por el mismo motivo.
fn pdf_encendido() -> bool {
    !std::env::var("VC_SIN_PDF").map(|v| v != "0" && !v.is_empty()).unwrap_or(false)
}

/// Como `flujos`, pero incluyendo los miembros ALMACENADOS de un zip.
///
/// Existe aparte de `flujos` a proposito: `flujos` promete "esto es deflate y se recrea con
/// preflate", y meter ahi regiones que no lo son romperia a todos sus llamadores en silencio.
pub fn regiones(d: &[u8]) -> Option<Vec<Region>> {
    match firma(d) {
        Formato::Zip => zip_regiones(d),
        _ => flujos(d).map(|v| v.into_iter().map(|t| Region { tramos: t, crudo: false }).collect()),
    }
}

// ---------- gzip ----------

/// Cuerpo DEFLATE de un gzip de UN miembro (los multi-miembro no entran).
fn gzip(d: &[u8]) -> Option<Flujo> {
    if d.len() < 20 { return None; }
    let flg = d[3];
    let mut p = 10usize;
    if flg & 4 != 0 {
        if p + 2 > d.len() { return None; }
        p += 2 + u16::from_le_bytes([d[p], d[p + 1]]) as usize;
    }
    if flg & 8 != 0 { while p < d.len() && d[p] != 0 { p += 1; } p += 1; }
    if flg & 16 != 0 { while p < d.len() && d[p] != 0 { p += 1; } p += 1; }
    if flg & 2 != 0 { p += 2; }
    let fin = d.len().checked_sub(8)?;
    if p >= fin { return None; }
    Some(vec![(p as u32, (fin - p) as u32)])
}

// ---------- png ----------

/// Los IDAT concatenados son UN flujo zlib. Se deja fuera la cabecera zlib (2 bytes) y el
/// adler32 del final (4), que se quedan en el esqueleto y vuelven literales.
fn png(d: &[u8]) -> Option<Vec<Flujo>> {
    let mut p = 8usize;
    let mut tramos: Vec<(u32, u32)> = Vec::new();
    while p + 8 <= d.len() {
        let len = u32::from_be_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]]) as usize;
        let tipo = &d[p + 4..p + 8];
        let ini = p + 8;
        if ini.checked_add(len)?.checked_add(4)? > d.len() { return None; }
        if tipo == b"IDAT" && len > 0 { tramos.push((ini as u32, len as u32)); }
        if tipo == b"IEND" { break; }
        p = ini + len + 4;
    }
    if tramos.is_empty() { return None; }
    // recortar los 2 bytes de cabecera zlib del principio y los 4 del adler del final
    recorta(&mut tramos, 2, 4)?;
    Some(vec![tramos])
}

/// Quita `ini` bytes del principio y `fin` del final de una lista de tramos.
fn recorta(t: &mut Vec<(u32, u32)>, ini: u32, fin: u32) -> Option<()> {
    let total: u64 = t.iter().map(|&(_, l)| l as u64).sum();
    if total <= (ini + fin) as u64 { return None; }
    let mut quitar = ini;
    while quitar > 0 {
        let (o, l) = t[0];
        let n = quitar.min(l);
        if n == l { t.remove(0); } else { t[0] = (o + n, l - n); }
        quitar -= n;
    }
    let mut quitar = fin;
    while quitar > 0 {
        let i = t.len() - 1;
        let (o, l) = t[i];
        let n = quitar.min(l);
        if n == l { t.pop(); } else { t[i] = (o, l - n); }
        quitar -= n;
    }
    if t.is_empty() { None } else { Some(()) }
}

// ---------- pdf ----------

/// Los flujos `/FlateDecode` de un PDF.
///
/// 🔑 POR QUE EXISTE. `firma()` reconocia tres formatos y el PDF caia en el `_ => None`: iba al
/// carril media, a zstd, COMO OPACO. Medido sobre 200 PDF reales del corpus
/// (`bench/contenedores.py`, 167,3 MiB): el **54,5%** de sus bytes son DEFLATE que este carril
/// sabe deshacer —mediana del 76,6% por fichero— y no se tocaban nunca. Es la bolsa de bytes mas
/// grande que quedaba sin abrir.
///
/// ⚠️ EL 35,8% RESTANTE NO ES NUESTRO y no se cuenta como si lo fuera: son DCTDecode (JPEG, que
/// ya trata el carril JXL), JPX y fax. Contar todos los flujos juntos daria una cifra bonita y
/// un carril que luego no rinde.
///
/// COMO SE ACOTA CADA FLUJO, y por que en este orden:
///   1. `/Length` DIRECTO si lo hay — es el unico dato exacto del fichero.
///   2. si no, se busca `endstream` y se recortan los blancos de delante. Un `endstream` puede
///      salir por casualidad dentro de datos binarios; no hace falta blindarlo aqui porque
///      `preflate_seguro` REHACE el flujo y lo compara byte a byte (`lib.rs:659`): un tramo mal
///      acotado no entra, solo se pierde su ganancia.
///   3. la cabecera zlib se COMPRUEBA (CMF/FLG, RFC 1950). Sin esto, un productor que emita
///      deflate crudo —los hay— haria que recortar dos bytes destrozara el flujo en silencio.
fn pdf(d: &[u8]) -> Option<Vec<Flujo>> {
    let mut out: Vec<Flujo> = Vec::new();
    let mut p = 0usize;
    while let Some(rel) = buscar(&d[p..], b"stream") {
        let i = p + rel;
        p = i + 6;
        // `endstream` tambien contiene «stream»: sin esto se procesaria dos veces cada objeto.
        if i >= 3 && &d[i - 3..i] == b"end" { continue }
        // El diccionario del objeto va justo antes. 512 bytes cubren los de verdad; uno mas
        // largo que eso se deja pasar en vez de adivinar.
        let cab = &d[i.saturating_sub(512)..i];
        if !flate_primero(cab) { continue }
        // Tras `stream` va CRLF o LF (no CR suelto, lo dice la norma).
        let mut ini = i + 6;
        if d.get(ini) == Some(&b'\r') { ini += 1 }
        if d.get(ini) == Some(&b'\n') { ini += 1 }
        // El final: `/Length` directo si lo hay; si no, `endstream`.
        let fin = match longitud_directa(cab) {
            Some(n) if ini.checked_add(n).map(|f| f <= d.len()).unwrap_or(false) => ini + n,
            _ => {
                let Some(e) = buscar(&d[ini..], b"endstream") else { continue };
                let mut f = ini + e;
                while f > ini && matches!(d[f - 1], b'\n' | b'\r' | b' ') { f -= 1 }
                f
            }
        };
        // 2 de cabecera + 4 de adler + algo que despiezar. Por debajo de esto no hay carril.
        if fin <= ini + 16 { continue }
        if !cabecera_zlib(d[ini], d[ini + 1]) { continue }
        let mut tramos = vec![(ini as u32, (fin - ini) as u32)];
        // Igual que en PNG: la cabecera zlib y el adler se quedan en el esqueleto, literales.
        if recorta(&mut tramos, 2, 4).is_none() { continue }
        out.push(tramos);
        p = fin;
    }
    if out.is_empty() { return None }
    // Mismo contrato que el zip: en orden y sin solaparse, o el esqueleto no se recompone.
    out.sort_by_key(|t| t[0].0);
    for w in out.windows(2) {
        let (o, l) = w[0][0];
        if o + l > w[1][0].0 { return None }
    }
    Some(out)
}

fn buscar(h: &[u8], aguja: &[u8]) -> Option<usize> {
    h.windows(aguja.len()).position(|w| w == aguja)
}

/// ¿Es `FlateDecode` el PRIMER filtro del diccionario?
///
/// El primero es el que hay que deshacer primero (PDF 32000-1, 7.4.1), asi que en
/// `/Filter [/FlateDecode /DCTDecode]` la capa de fuera SI es nuestra: deshacerla deja un JPEG,
/// que es exactamente lo que queremos. Lo que no vale es que el primero sea otro.
fn flate_primero(cab: &[u8]) -> bool {
    let Some(f) = cab.windows(7).rposition(|w| w == b"/Filter") else { return false };
    let mut j = f + 7;
    while j < cab.len() && matches!(cab[j], b' ' | b'\r' | b'\n' | b'\t' | b'[') { j += 1 }
    cab[j..].starts_with(b"/FlateDecode")
}

/// `/Length 1234` con el valor DIRECTO. `/Length 5 0 R` es una referencia indirecta —el numero
/// vive en otro objeto— y aqui se devuelve `None` a proposito: leerla obligaria a resolver la
/// tabla de objetos, y el respaldo (`endstream`) ya cubre esos casos.
fn longitud_directa(cab: &[u8]) -> Option<usize> {
    let f = cab.windows(7).rposition(|w| w == b"/Length")?;
    let mut j = f + 7;
    while j < cab.len() && matches!(cab[j], b' ' | b'\r' | b'\n' | b'\t') { j += 1 }
    let ini = j;
    while j < cab.len() && cab[j].is_ascii_digit() { j += 1 }
    if j == ini { return None }
    let n: usize = std::str::from_utf8(&cab[ini..j]).ok()?.parse().ok()?;
    // Lo que sigue decide si el numero era el valor o el primero de `5 0 R`.
    let mut k = j;
    while k < cab.len() && matches!(cab[k], b' ' | b'\r' | b'\n' | b'\t') { k += 1 }
    if cab.get(k).is_some_and(|c| c.is_ascii_digit()) { return None }
    Some(n)
}

/// La cabecera de zlib del RFC 1950: metodo 8, ventana <= 32 KiB, el par multiplo de 31 y sin
/// diccionario preestablecido.
fn cabecera_zlib(cmf: u8, flg: u8) -> bool {
    cmf & 0x0F == 8 && (cmf >> 4) <= 7 && flg & 0x20 == 0
        && (u16::from(cmf) * 256 + u16::from(flg)) % 31 == 0
}

// ---------- zip (y todo lo que es un zip por dentro: docx, xlsx, pptx, jar, apk, epub) ----------

/// Se recorre el DIRECTORIO CENTRAL, no las cabeceras locales: es donde los tamaños son
/// fiables (con descriptor de datos, la cabecera local los trae a cero).
fn zip_regiones(d: &[u8]) -> Option<Vec<Region>> {
    // EOCD: firma PK\5\6 buscando desde el final (el comentario puede medir hasta 64 KiB)
    let min = d.len().saturating_sub(65536 + 22);
    let mut eocd = None;
    let mut i = d.len().checked_sub(22)?;
    loop {
        if d[i..i + 4] == [b'P', b'K', 5, 6] { eocd = Some(i); break; }
        if i == min { break; }
        i -= 1;
    }
    let e = eocd?;
    let n = u16::from_le_bytes([d[e + 10], d[e + 11]]) as usize;
    let mut cd = u32::from_le_bytes([d[e + 16], d[e + 17], d[e + 18], d[e + 19]]) as usize;
    if n == 0 || cd >= d.len() { return None; }

    let mut out = Vec::new();
    for _ in 0..n {
        if cd + 46 > d.len() || d[cd..cd + 4] != [b'P', b'K', 1, 2] { return None; }
        let metodo = u16::from_le_bytes([d[cd + 10], d[cd + 11]]);
        let csize = u32::from_le_bytes([d[cd + 20], d[cd + 21], d[cd + 22], d[cd + 23]]) as usize;
        let nlen = u16::from_le_bytes([d[cd + 28], d[cd + 29]]) as usize;
        let xlen = u16::from_le_bytes([d[cd + 30], d[cd + 31]]) as usize;
        let clen = u16::from_le_bytes([d[cd + 32], d[cd + 33]]) as usize;
        let lho = u32::from_le_bytes([d[cd + 42], d[cd + 43], d[cd + 44], d[cd + 45]]) as usize;
        cd += 46 + nlen + xlen + clen;

        // 8 = deflate, 0 = almacenado. Lo almacenado NO se salta: en ofimatica es donde
        // viven las imagenes -97,8% de los bytes de PNG- y saltarlo era dejar 18 MB de 81
        // como bytes literales que nadie mira.
        if csize == 0 { continue; }
        let crudo = match metodo { 8 => false, 0 => true, _ => continue };
        if csize == u32::MAX as usize { return None; } // zip64: fuera por ahora
        // El tamaño de los campos variables se lee de la cabecera LOCAL: puede no coincidir
        // con la central, y es la local la que marca dónde empiezan los datos.
        if lho + 30 > d.len() || d[lho..lho + 4] != [b'P', b'K', 3, 4] { return None; }
        let lnlen = u16::from_le_bytes([d[lho + 26], d[lho + 27]]) as usize;
        let lxlen = u16::from_le_bytes([d[lho + 28], d[lho + 29]]) as usize;
        let ini = lho + 30 + lnlen + lxlen;
        if ini.checked_add(csize)? > d.len() { return None; }
        out.push(Region { tramos: vec![(ini as u32, csize as u32)], crudo });
    }
    if out.is_empty() { return None; }
    // Deben ir en orden y sin solaparse: si no, el esqueleto no se puede reconstruir.
    out.sort_by_key(|r| r.tramos[0].0);
    for w in out.windows(2) {
        let (o, l) = w[0].tramos[0];
        if o + l > w[1].tramos[0].0 { return None; }
    }
    Some(out)
}


/// Solo los flujos DEFLATE de un zip. Se deriva de `zip_regiones` en vez de repetir el
/// analisis: dos analizadores del mismo formato acaban divergiendo, y el que se usa menos es
/// el que se queda con el fallo.
fn zip(d: &[u8]) -> Option<Vec<Flujo>> {
    let v: Vec<Flujo> = zip_regiones(d)?.into_iter().filter(|r| !r.crudo).map(|r| r.tramos).collect();
    if v.is_empty() { None } else { Some(v) }
}
