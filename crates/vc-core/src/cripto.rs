// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! Cifrado del archivo. ChaCha20-Poly1305 con clave derivada por Argon2id.
//!
//! POR QUE HACE FALTA. restic y borg cifran POR DEFECTO. Un respaldo sin cifrar no se sube a la
//! nube de nadie, y la nube es donde vive el 90% de los respaldos. Sin esto, todo lo que
//! comprimimos mejor que nadie no sirve para el caso que importa.
//!
//! LAS DECISIONES, y por que cada una:
//!
//! CHACHA20-POLY1305 y no AES-GCM. Es AEAD -cifra Y autentica-, va rapido sin instrucciones de
//! CPU especiales, y no tiene el filo de AES-GCM con los nonces. Que sea AEAD es lo que importa:
//! un cifrado sin autenticar deja que alguien modifique el archivo sin que se note, y en un
//! respaldo eso es peor que no cifrar, porque restaurarias basura creyendola buena.
//!
//! ARGON2ID y no un hash pelado. Una contrasena tiene poca entropia; derivarla con SHA-256 la
//! deja al alcance de una GPU. Argon2id esta hecho para costar memoria, que es lo que las GPU no
//! tienen de sobra. Parametros por defecto: 64 MiB, 3 pasadas.
//!
//! EL NONCE VIAJA CON EL FRAME, no en su registro del indice. Es la decision que mas importa:
//! `forget` compacta el archivo RENUMERANDO los frames, y si el nonce dependiera del numero se
//! reutilizaria con la misma clave sobre datos distintos — que es la forma clasica de romper
//! ChaCha20 del todo. Guardandolo en los primeros 12 bytes del propio frame, la compactacion lo
//! copia tal cual y no hay nada que renumerar. Ademas no cambia ninguna estructura del indice.
//!
//! NO SE ACEPTA LA CONTRASENA COMO ARGUMENTO. Un `--password loquesea` sale en `ps` para
//! cualquier usuario de la maquina y se queda en el historial del shell. Se lee de un fichero o
//! de una variable de entorno, y se dice por que.

use anyhow::{bail, Context, Result};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};

/// Bytes que el cifrado anade a cada bloque: 12 de nonce + 16 de etiqueta de autenticacion.
pub const SOBRECOSTE: usize = 12 + 16;
/// Longitud de la sal de derivacion.
pub const SAL: usize = 16;

/// Los parametros de Argon2id. Se GUARDAN en el archivo: si algun dia se suben, los archivos
/// viejos tienen que seguir abriendose con los suyos.
#[derive(Clone, Copy)]
pub struct Params { pub mib: u32, pub pasadas: u32, pub carriles: u32 }
impl Default for Params {
    fn default() -> Self { Params { mib: 64, pasadas: 3, carriles: 1 } }
}

/// Bytes aleatorios de verdad, del nucleo.
///
/// Se lee /dev/urandom en vez de arrastrar otra dependencia: es la fuente correcta en Linux y
/// una linea de codigo que cualquiera puede auditar. Si falla, se ABORTA: seguir con numeros
/// predecibles seria cifrar de mentira.
pub fn azar(n: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut v = vec![0u8; n];
    std::fs::File::open("/dev/urandom")
        .context("abrir /dev/urandom para generar aleatorios")?
        .read_exact(&mut v)
        .context("leer aleatorios")?;
    Ok(v)
}

/// Deriva la clave de la contrasena. Es lo lento a proposito: es lo que hace que probar
/// contrasenas a lo bruto cueste dinero.
pub fn deriva(pass: &[u8], sal: &[u8], p: Params) -> Result<[u8; 32]> {
    use argon2::{Algorithm, Argon2, ParamsBuilder, Version};
    let params = ParamsBuilder::new()
        .m_cost(p.mib * 1024)
        .t_cost(p.pasadas)
        .p_cost(p.carriles)
        .output_len(32)
        .build()
        .map_err(|e| anyhow::anyhow!("parámetros de Argon2 inválidos: {e}"))?;
    let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut clave = [0u8; 32];
    a2.hash_password_into(pass, sal, &mut clave)
        .map_err(|e| anyhow::anyhow!("derivar la clave: {e}"))?;
    Ok(clave)
}

/// Cifra un bloque. Devuelve nonce ‖ ciphertext ‖ etiqueta.
///
/// El nonce es aleatorio y va DELANTE, dentro del propio bloque. Con 96 bits y una clave nueva
/// por archivo, la probabilidad de que dos frames repitan nonce es despreciable incluso con
/// miles de millones de frames.
pub fn cifra(clave: &[u8; 32], claro: &[u8]) -> Result<Vec<u8>> {
    let c = ChaCha20Poly1305::new(Key::from_slice(clave));
    let n = azar(12)?;
    let nonce = Nonce::from_slice(&n);
    let mut out = Vec::with_capacity(claro.len() + SOBRECOSTE);
    out.extend_from_slice(&n);
    out.extend_from_slice(&c.encrypt(nonce, claro)
        .map_err(|_| anyhow::anyhow!("fallo al cifrar"))?);
    Ok(out)
}

/// Descifra un bloque. Falla si la contrasena no es la buena O si alguien ha tocado los bytes:
/// las dos cosas son el mismo fallo de autenticacion y no se pueden distinguir — a proposito,
/// porque distinguirlas le diria a un atacante cuando ha acertado la contrasena.
pub fn descifra(clave: &[u8; 32], bloque: &[u8]) -> Result<Vec<u8>> {
    if bloque.len() < SOBRECOSTE {
        bail!("bloque cifrado demasiado corto ({} B): archivo corrupto", bloque.len());
    }
    let c = ChaCha20Poly1305::new(Key::from_slice(clave));
    let nonce = Nonce::from_slice(&bloque[..12]);
    c.decrypt(nonce, &bloque[12..])
        .map_err(|_| anyhow::anyhow!("no se puede descifrar: la contraseña no es correcta, o el \
                                      archivo ha sido modificado"))
}

/// De donde sale la contrasena. NUNCA de un argumento de linea de comandos.
///
/// `--password X` seria comodo y saldria en `ps` para cualquier usuario de la maquina, ademas de
/// quedarse en el historial del shell. Las dos vias que quedan son un fichero -que se puede
/// proteger con permisos- y una variable de entorno, que es lo que usan los sistemas de
/// despliegue.
/// `confirmar`: pedirla dos veces. Solo al CREAR un archivo, donde equivocarse deja algo que
/// nadie podra abrir nunca.
pub fn contrasena(fichero: Option<&str>, confirmar: bool) -> Result<Vec<u8>> {
    if let Some(f) = fichero {
        let v = std::fs::read(f).with_context(|| format!("leer la contraseña de {f}"))?;
        // Un fichero escrito con un editor acaba en salto de linea y el usuario no lo sabe.
        let v: Vec<u8> = v.into_iter().rev().skip_while(|&b| b == b'\n' || b == b'\r')
            .collect::<Vec<u8>>().into_iter().rev().collect();
        if v.is_empty() { bail!("el fichero de contraseña {f} está vacío"); }
        return Ok(v);
    }
    if let Ok(v) = std::env::var("VC_PASSWORD") {
        if !v.is_empty() { return Ok(v.into_bytes()); }
    }
    // Si hay un terminal, se PIDE. Sin esto, la unica forma de cifrar a mano era crear un
    // fichero de contrasena primero, que para una persona sentada delante es absurdo — y la
    // clase de friccion que hace que la gente acabe no cifrando.
    if unsafe { libc::isatty(0) } == 1 {
        return pide_por_terminal(confirmar);
    }
    bail!("hace falta una contraseña: usa --password-file <fichero>, la variable de entorno \
           VC_PASSWORD, o ejecuta esto desde un terminal para que te la pida.\n\nNo se acepta \
           como argumento de línea de comandos a propósito: sería visible en `ps` para cualquier \
           usuario de la máquina y quedaría en el historial del shell.")
}

/// Pide la contrasena por el terminal, SIN eco.
///
/// Se apaga el eco con termios y se vuelve a encender pase lo que pase —incluido un error a
/// mitad—. Dejar un terminal sin eco despues de fallar es de las cosas mas desagradables que
/// puede hacer un programa: el usuario se queda escribiendo a ciegas sin saber por que.
fn pide_por_terminal(confirmar: bool) -> Result<Vec<u8>> {
    use std::io::{BufRead, Write};

    let mut t: libc::termios = unsafe { std::mem::zeroed() };
    let hay_termios = unsafe { libc::tcgetattr(0, &mut t) } == 0;
    let original = t;
    if hay_termios {
        t.c_lflag &= !libc::ECHO;
        unsafe { libc::tcsetattr(0, libc::TCSANOW, &t) };
    }
    let restaura = || { if hay_termios { unsafe { libc::tcsetattr(0, libc::TCSANOW, &original) }; } };

    let leer = |aviso: &str| -> Result<Vec<u8>> {
        eprint!("{aviso}");
        let _ = std::io::stderr().flush();
        let mut linea = String::new();
        let n = std::io::stdin().lock().read_line(&mut linea)?;
        eprintln!();
        if n == 0 { bail!("no se leyó ninguna contraseña"); }
        Ok(linea.trim_end_matches(['\n', '\r']).as_bytes().to_vec())
    };

    let resultado = (|| -> Result<Vec<u8>> {
        let a = leer("Contraseña: ")?;
        if a.is_empty() { bail!("la contraseña está vacía"); }
        if confirmar {
            // Al CREAR se pide dos veces. Si no, un dedo torcido deja un archivo que nadie
            // podra abrir nunca — y no hay forma de recuperarlo, por diseño.
            let b = leer("Repítela: ")?;
            if a != b { bail!("las dos contraseñas no coinciden"); }
        }
        Ok(a)
    })();
    restaura();
    resultado
}

#[cfg(test)]
mod pruebas {
    use super::*;

    /// Parametros minimos: las pruebas no deben tardar 3 s cada una en derivar. La FUERZA de
    /// Argon2 se prueba por sus parametros, no reejecutandolo; lo que se prueba aqui es que el
    /// cableado es correcto.
    fn rapidos() -> Params { Params { mib: 8, pasadas: 1, carriles: 1 } }

    #[test]
    fn ida_y_vuelta() {
        let k = deriva(b"secreto", b"0123456789abcdef", rapidos()).unwrap();
        let claro = b"esto es un frame comprimido cualquiera".to_vec();
        let c = cifra(&k, &claro).unwrap();
        assert_ne!(c[12..], claro[..], "el ciphertext no puede parecerse al claro");
        assert_eq!(descifra(&k, &c).unwrap(), claro);
    }

    #[test]
    fn la_contrasena_equivocada_falla() {
        let sal = b"0123456789abcdef";
        let buena = deriva(b"secreto", sal, rapidos()).unwrap();
        let mala = deriva(b"secretp", sal, rapidos()).unwrap();
        let c = cifra(&buena, b"datos").unwrap();
        assert!(descifra(&mala, &c).is_err(), "una contraseña equivocada NO puede descifrar");
    }

    #[test]
    fn tocar_un_byte_lo_invalida() {
        // Esto es lo que separa un cifrado autenticado de uno que no lo es. Sin esto, alguien
        // podria modificar el respaldo y lo restaurarias creyendolo bueno.
        let k = deriva(b"secreto", b"0123456789abcdef", rapidos()).unwrap();
        let c = cifra(&k, b"datos importantes").unwrap();
        for i in [0usize, 13, 20] {
            let mut roto = c.clone();
            roto[i] ^= 1;
            assert!(descifra(&k, &roto).is_err(), "cambiar el byte {i} tiene que invalidarlo");
        }
    }

    #[test]
    fn el_mismo_claro_da_ciphertext_distinto() {
        // Si dos cifrados del mismo dato salieran iguales, un observador sabria que dos frames
        // tienen el mismo contenido. El nonce aleatorio es lo que lo evita.
        let k = deriva(b"secreto", b"0123456789abcdef", rapidos()).unwrap();
        let a = cifra(&k, b"mismo dato").unwrap();
        let b = cifra(&k, b"mismo dato").unwrap();
        assert_ne!(a, b, "dos cifrados del mismo claro no pueden coincidir");
        assert_eq!(descifra(&k, &a).unwrap(), descifra(&k, &b).unwrap());
    }

    #[test]
    fn la_derivacion_es_determinista_y_depende_de_la_sal() {
        let p = rapidos();
        assert_eq!(deriva(b"x", b"0123456789abcdef", p).unwrap(),
                   deriva(b"x", b"0123456789abcdef", p).unwrap(),
                   "misma contraseña y sal tienen que dar la misma clave");
        assert_ne!(deriva(b"x", b"0123456789abcdef", p).unwrap(),
                   deriva(b"x", b"fedcba9876543210", p).unwrap(),
                   "la sal tiene que cambiar la clave: es lo que impide tablas precalculadas");
    }

    #[test]
    fn un_bloque_corto_no_revienta() {
        let k = deriva(b"secreto", b"0123456789abcdef", rapidos()).unwrap();
        for n in [0usize, 5, 27] {
            assert!(descifra(&k, &vec![0u8; n]).is_err(), "un bloque de {n} B debe dar error limpio");
        }
    }

    #[test]
    fn el_azar_no_se_repite() {
        assert_ne!(azar(32).unwrap(), azar(32).unwrap());
        assert_eq!(azar(32).unwrap().len(), 32);
    }
}
