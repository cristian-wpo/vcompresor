// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! Carril JPEG (F2.3) — transcodificación a JPEG XL con reconstrucción BYTE-EXACTA.
//!
//! No es "convertir a otro formato": libjxl guarda el `jbrd` (JPEG bitstream reconstruction
//! data) dentro del propio flujo JXL, de modo que el JPEG original se recrea bit a bit.
//! Igual que el carril preflate, la recreación se VERIFICA en el empaquetado: si no sale
//! byte-idéntica, el fichero no entra por este carril y cae al camino normal (store).
//!
//! FFI directa contra libjxl (declaraciones tomadas de /usr/include/jxl/{encode,decode}.h,
//! libjxl 0.7). Sin crate intermedio: el binario enlaza libjxl y el archivo se abre con
//! nuestra CLI, sin depender de que existan las herramientas `cjxl`/`djxl` en el sistema.

use std::os::raw::{c_int, c_void};

type JxlEncoder = c_void;
type JxlEncoderFrameSettings = c_void;
type JxlDecoder = c_void;

const ENC_SUCCESS: c_int = 0;
const ENC_NEED_MORE_OUTPUT: c_int = 2;
const ENC_FRAME_SETTING_EFFORT: c_int = 0;

const DEC_SUCCESS: c_int = 0;
const DEC_JPEG_NEED_MORE_OUTPUT: c_int = 6;
const DEC_FULL_IMAGE: c_int = 0x1000;
const DEC_JPEG_RECONSTRUCTION: c_int = 0x2000;

#[link(name = "jxl")]
extern "C" {
    fn JxlEncoderCreate(memory_manager: *const c_void) -> *mut JxlEncoder;
    fn JxlEncoderDestroy(enc: *mut JxlEncoder);
    fn JxlEncoderStoreJPEGMetadata(enc: *mut JxlEncoder, store_jpeg_metadata: c_int) -> c_int;
    fn JxlEncoderFrameSettingsCreate(
        enc: *mut JxlEncoder,
        source: *const JxlEncoderFrameSettings,
    ) -> *mut JxlEncoderFrameSettings;
    fn JxlEncoderFrameSettingsSetOption(
        frame_settings: *mut JxlEncoderFrameSettings,
        option: c_int,
        value: i64,
    ) -> c_int;
    fn JxlEncoderAddJPEGFrame(
        frame_settings: *const JxlEncoderFrameSettings,
        buffer: *const u8,
        size: usize,
    ) -> c_int;
    fn JxlEncoderCloseInput(enc: *mut JxlEncoder);
    fn JxlEncoderProcessOutput(
        enc: *mut JxlEncoder,
        next_out: *mut *mut u8,
        avail_out: *mut usize,
    ) -> c_int;

    fn JxlDecoderCreate(memory_manager: *const c_void) -> *mut JxlDecoder;
    fn JxlDecoderDestroy(dec: *mut JxlDecoder);
    fn JxlDecoderSubscribeEvents(dec: *mut JxlDecoder, events_wanted: c_int) -> c_int;
    fn JxlDecoderSetInput(dec: *mut JxlDecoder, data: *const u8, size: usize) -> c_int;
    fn JxlDecoderCloseInput(dec: *mut JxlDecoder);
    fn JxlDecoderProcessInput(dec: *mut JxlDecoder) -> c_int;
    fn JxlDecoderSetJPEGBuffer(dec: *mut JxlDecoder, data: *mut u8, size: usize) -> c_int;
    fn JxlDecoderReleaseJPEGBuffer(dec: *mut JxlDecoder) -> usize;
}

struct Enc(*mut JxlEncoder);
impl Drop for Enc {
    fn drop(&mut self) {
        unsafe { JxlEncoderDestroy(self.0) }
    }
}
struct Dec(*mut JxlDecoder);
impl Drop for Dec {
    fn drop(&mut self) {
        unsafe { JxlDecoderDestroy(self.0) }
    }
}

/// Un JPEG válido empieza por SOI (FFD8) y termina en EOI (FFD9).
pub fn parece_jpeg(b: &[u8]) -> bool {
    b.len() > 4 && b[0] == 0xFF && b[1] == 0xD8 && b[b.len() - 2] == 0xFF && b[b.len() - 1] == 0xD9
}

/// JPEG → JXL con metadatos de reconstrucción. `None` si libjxl no puede transcodificarlo
/// (JPEG raro, aritmético, truncado…) — el llamante cae al camino normal.
pub fn jpeg_a_jxl(jpeg: &[u8], esfuerzo: i64) -> Option<Vec<u8>> {
    if !parece_jpeg(jpeg) {
        return None;
    }
    unsafe {
        let enc = Enc(JxlEncoderCreate(std::ptr::null()));
        if enc.0.is_null() {
            return None;
        }
        // Sin esto NO hay reconstrucción byte-exacta: es la condición del carril.
        if JxlEncoderStoreJPEGMetadata(enc.0, 1) != ENC_SUCCESS {
            return None;
        }
        let fs = JxlEncoderFrameSettingsCreate(enc.0, std::ptr::null());
        if fs.is_null() {
            return None;
        }
        JxlEncoderFrameSettingsSetOption(fs, ENC_FRAME_SETTING_EFFORT, esfuerzo);
        if JxlEncoderAddJPEGFrame(fs, jpeg.as_ptr(), jpeg.len()) != ENC_SUCCESS {
            return None;
        }
        JxlEncoderCloseInput(enc.0);

        let mut out = vec![0u8; jpeg.len().max(64 * 1024)];
        let mut escrito = 0usize;
        loop {
            let mut next = out.as_mut_ptr().add(escrito);
            let mut avail = out.len() - escrito;
            let st = JxlEncoderProcessOutput(enc.0, &mut next, &mut avail);
            escrito = out.len() - avail;
            match st {
                ENC_SUCCESS => {
                    out.truncate(escrito);
                    return Some(out);
                }
                ENC_NEED_MORE_OUTPUT => {
                    let n = out.len();
                    out.resize(n * 2, 0);
                }
                _ => return None,
            }
        }
    }
}

/// JXL (con jbrd) → el JPEG original, byte a byte. `pista` = tamaño esperado, para no
/// reasignar de más. `None` si el flujo no lleva datos de reconstrucción o está corrupto.
pub fn jxl_a_jpeg(jxl: &[u8], pista: usize) -> Option<Vec<u8>> {
    unsafe {
        let dec = Dec(JxlDecoderCreate(std::ptr::null()));
        if dec.0.is_null() {
            return None;
        }
        if JxlDecoderSubscribeEvents(dec.0, DEC_JPEG_RECONSTRUCTION | DEC_FULL_IMAGE) != DEC_SUCCESS
        {
            return None;
        }
        if JxlDecoderSetInput(dec.0, jxl.as_ptr(), jxl.len()) != DEC_SUCCESS {
            return None;
        }
        JxlDecoderCloseInput(dec.0);

        let mut buf = vec![0u8; pista.max(64 * 1024) + 4096];
        let mut visto_recon = false;
        loop {
            match JxlDecoderProcessInput(dec.0) {
                DEC_JPEG_RECONSTRUCTION => {
                    visto_recon = true;
                    if JxlDecoderSetJPEGBuffer(dec.0, buf.as_mut_ptr(), buf.len()) != DEC_SUCCESS {
                        return None;
                    }
                }
                DEC_JPEG_NEED_MORE_OUTPUT => {
                    // Ojo al orden: soltar el buffer ANTES de redimensionar (el resize puede
                    // reubicar la memoria y libjxl aún tendría el puntero viejo).
                    let restante = JxlDecoderReleaseJPEGBuffer(dec.0);
                    let usado = buf.len() - restante;
                    let n = buf.len();
                    buf.resize(n * 2, 0);
                    if JxlDecoderSetJPEGBuffer(dec.0, buf.as_mut_ptr().add(usado), buf.len() - usado)
                        != DEC_SUCCESS
                    {
                        return None;
                    }
                }
                DEC_FULL_IMAGE | DEC_SUCCESS => {
                    if !visto_recon {
                        return None; // no llevaba jbrd: no reconstruye el original
                    }
                    let restante = JxlDecoderReleaseJPEGBuffer(dec.0);
                    let total = buf.len() - restante;
                    buf.truncate(total);
                    return Some(buf);
                }
                _ => return None,
            }
        }
    }
}

/// Transcodifica y COMPRUEBA la vuelta. Devuelve el JXL solo si reconstruye el JPEG
/// byte a byte y además ocupa menos que el original. Es la única puerta del carril.
pub fn jxl_verificado(jpeg: &[u8], esfuerzo: i64) -> Option<Vec<u8>> {
    let jxl = jpeg_a_jxl(jpeg, esfuerzo)?;
    if jxl.len() >= jpeg.len() {
        return None; // no engordamos: que vaya por el camino normal
    }
    let vuelta = jxl_a_jpeg(&jxl, jpeg.len())?;
    if vuelta != jpeg {
        return None; // paranoia: si no es byte-exacto, NO entra
    }
    Some(jxl)
}
