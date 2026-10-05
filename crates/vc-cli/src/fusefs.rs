// SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-vcompresor-Commercial
// Copyright (c) 2026 Vértice Labs. Ver LICENSE y LICENSE-COMMERCIAL.md

//! F2.4 — montar un archivo .vc como carpeta de SOLO LECTURA (FUSE).
//! Lee O(bloque): cada read descomprime como mucho los frames que toca (cache LRU pequeña).
#![cfg(target_os = "linux")]

use fuser::{FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::ffi::OsStr;
use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};
use vc_core::{FrameRec, Index, Transform};

const TTL: Duration = Duration::from_secs(3600);
const FRAME_CACHE: usize = 8;
const MATERIAL_CACHE_MAX: usize = 512 * 1024 * 1024;

enum Node {
    Dir { children: BTreeMap<String, u64> },
    File { fidx: usize, size: u64 },
}

pub struct VcFs {
    idx: Index,
    arch: Mutex<File>,
    nodes: HashMap<u64, Node>,
    padres: HashMap<u64, u64>,
    // prefix-sums de offsets de chunk por fichero (acceso O(log n))
    starts: HashMap<usize, Vec<u64>>,
    fcache: Mutex<(VecDeque<u32>, HashMap<u32, Arc<Vec<u8>>>)>,
    // ficheros con transformada (gz): materializados enteros bajo demanda
    mat: Mutex<(usize, HashMap<usize, Arc<Vec<u8>>>)>,
    uid: u32,
    gid: u32,
}

impl VcFs {
    pub fn new(arch_path: &Path) -> anyhow::Result<Self> {
        let (idx, arch) = vc_core::abrir(arch_path)?;
        let mut nodes: HashMap<u64, Node> = HashMap::new();
        let mut padres: HashMap<u64, u64> = HashMap::new();
        nodes.insert(1, Node::Dir { children: BTreeMap::new() });
        let mut next: u64 = 2;
        let mut starts: HashMap<usize, Vec<u64>> = HashMap::new();
        for (fi, fr) in idx.files.iter().enumerate() {
            // tamaño visible = original (con transformada, el .gz reconstruido)
            let mut dir = 1u64;
            let comps: Vec<&str> = fr.path.split("/").filter(|s| !s.is_empty()).collect();
            for (i, comp) in comps.iter().enumerate() {
                let es_hoja = i + 1 == comps.len();
                let entry = {
                    let Node::Dir { children } = nodes.get_mut(&dir).unwrap() else { unreachable!() };
                    children.get(*comp).copied()
                };
                let ino = match entry {
                    Some(e) => e,
                    None => {
                        let ino = next; next += 1;
                        let Node::Dir { children } = nodes.get_mut(&dir).unwrap() else { unreachable!() };
                        children.insert((*comp).to_string(), ino);
                        padres.insert(ino, dir);
                        nodes.insert(ino, if es_hoja { Node::File { fidx: fi, size: fr.size } } else { Node::Dir { children: BTreeMap::new() } });
                        ino
                    }
                };
                dir = ino;
            }
            let mut acc = 0u64;
            let mut v = Vec::with_capacity(fr.chunks.len() + 1);
            for &cid in &fr.chunks { v.push(acc); acc += idx.chunks[cid as usize].len as u64; }
            v.push(acc);
            starts.insert(fi, v);
        }
        Ok(VcFs {
            idx, arch: Mutex::new(arch), nodes, padres, starts,
            fcache: Mutex::new((VecDeque::new(), HashMap::new())),
            mat: Mutex::new((0, HashMap::new())),
            uid: unsafe { libc::getuid() }, gid: unsafe { libc::getgid() },
        })
    }

    fn attr(&self, ino: u64) -> Option<FileAttr> {
        let n = self.nodes.get(&ino)?;
        let (kind, size, perm, nlink) = match n {
            Node::Dir { .. } => (FileType::Directory, 0, 0o555, 2),
            Node::File { size, .. } => (FileType::RegularFile, *size, 0o444, 1),
        };
        let t = SystemTime::UNIX_EPOCH;
        Some(FileAttr { ino, size, blocks: size.div_ceil(512), atime: t, mtime: t, ctime: t, crtime: t,
            kind, perm, nlink, uid: self.uid, gid: self.gid, rdev: 0, blksize: 128 * 1024, flags: 0 })
    }

    fn frame(&self, id: u32) -> anyhow::Result<Arc<Vec<u8>>> {
        {
            let g = self.fcache.lock().unwrap();
            if let Some(v) = g.1.get(&id) { return Ok(Arc::clone(v)); }
        }
        let fr: &FrameRec = &self.idx.frames[id as usize];
        let data = {
            let mut f = self.arch.lock().unwrap();
            use std::io::{Read, Seek, SeekFrom};
            f.seek(SeekFrom::Start(fr.offset))?;
            let mut comp = vec![0u8; fr.csize as usize];
            f.read_exact(&mut comp)?;
            vc_core::descodifica_frame(comp, fr)?
        };
        let a = Arc::new(data);
        let mut g = self.fcache.lock().unwrap();
        g.0.push_back(id); g.1.insert(id, Arc::clone(&a));
        while g.0.len() > FRAME_CACHE { if let Some(old) = g.0.pop_front() { g.1.remove(&old); } }
        Ok(a)
    }

    fn leer_plano(&self, fidx: usize, off: u64, size: u32) -> anyhow::Result<Vec<u8>> {
        let fr = &self.idx.files[fidx];
        let st = &self.starts[&fidx];
        let total = *st.last().unwrap_or(&0);
        if off >= total { return Ok(Vec::new()); }
        let fin = (off + size as u64).min(total);
        let mut out = Vec::with_capacity((fin - off) as usize);
        let mut i = st.partition_point(|&s| s <= off) - 1;
        let mut pos = off;
        while pos < fin && i < fr.chunks.len() {
            let c = &self.idx.chunks[fr.chunks[i] as usize];
            let cini = st[i];
            let dentro = (pos - cini) as usize;
            let take = ((fin - pos) as usize).min(c.len as usize - dentro);
            let fdata = self.frame(c.frame)?;
            let base = c.off as usize + dentro;
            out.extend_from_slice(&fdata[base..base + take]);
            pos += take as u64;
            i += 1;
        }
        Ok(out)
    }

    /// Corta [off, off+size) de un fichero ya materializado en la cache; None si no está.
    fn corte_cacheado(&self, fidx: usize, off: u64, size: u32) -> Option<Vec<u8>> {
        let g = self.mat.lock().unwrap();
        let v = g.1.get(&fidx)?;
        let fin = ((off + size as u64) as usize).min(v.len());
        Some(if (off as usize) < fin { v[off as usize..fin].to_vec() } else { Vec::new() })
    }

    /// Mete el fichero reconstruido en la cache (vaciándola si se pasa) y devuelve el corte.
    fn materializa(&self, fidx: usize, v: Vec<u8>, off: u64, size: u32) -> Vec<u8> {
        let a = Arc::new(v);
        let mut g = self.mat.lock().unwrap();
        g.0 += a.len();
        if g.0 > MATERIAL_CACHE_MAX { g.1.clear(); g.0 = a.len(); }
        g.1.insert(fidx, Arc::clone(&a));
        let fin = ((off + size as u64) as usize).min(a.len());
        if (off as usize) < fin { a[off as usize..fin].to_vec() } else { Vec::new() }
    }

    fn leer(&self, fidx: usize, off: u64, size: u32) -> anyhow::Result<Vec<u8>> {
        match &self.idx.files[fidx].transform {
            None => self.leer_plano(fidx, off, size),
            Some(Transform::Deflate { esqueleto, huecos }) => {
                if let Some(v) = self.corte_cacheado(fidx, off, size) { return Ok(v); }
                let plano = self.leer_plano(fidx, 0, u32::MAX)?;
                let v = vc_core::rehace_deflate_pub(esqueleto, huecos, &plano)?;
                Ok(self.materializa(fidx, v, off, size))
            }
            Some(Transform::LogTs) => {
                if let Some(v) = self.corte_cacheado(fidx, off, size) { return Ok(v); }
                let plano = self.leer_plano(fidx, 0, u32::MAX)?;
                let v = vc_core::log_ts_inv(&plano)
                    .ok_or_else(|| anyhow::anyhow!("log-ts: reconstrucción fallida"))?;
                Ok(self.materializa(fidx, v, off, size))
            }
            Some(Transform::E8e9) => {
                if let Some(v) = self.corte_cacheado(fidx, off, size) { return Ok(v); }
                let mut v = self.leer_plano(fidx, 0, u32::MAX)?;
                vc_core::e8e9(&mut v, false);
                Ok(self.materializa(fidx, v, off, size))
            }
            Some(Transform::JpegJxl) => {
                if let Some(v) = self.corte_cacheado(fidx, off, size) { return Ok(v); }
                let flujo = self.leer_plano(fidx, 0, u32::MAX)?;
                let jpeg = vc_core::jxl::jxl_a_jpeg(&flujo, self.idx.files[fidx].size as usize)
                    .ok_or_else(|| anyhow::anyhow!("reconstrucción JXL fallida"))?;
                Ok(self.materializa(fidx, jpeg, off, size))
            }
        }
    }
}

impl Filesystem for VcFs {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let Some(Node::Dir { children }) = self.nodes.get(&parent) else { reply.error(libc::ENOENT); return };
        match name.to_str().and_then(|n| children.get(n)) {
            Some(&ino) => match self.attr(ino) { Some(a) => reply.entry(&TTL, &a, 0), None => reply.error(libc::ENOENT) },
            None => reply.error(libc::ENOENT),
        }
    }
    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        match self.attr(ino) { Some(a) => reply.attr(&TTL, &a), None => reply.error(libc::ENOENT) }
    }
    fn read(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, size: u32, _flags: i32, _lock: Option<u64>, reply: ReplyData) {
        let Some(Node::File { fidx, .. }) = self.nodes.get(&ino) else { reply.error(libc::ENOENT); return };
        match self.leer(*fidx, offset.max(0) as u64, size) {
            Ok(v) => reply.data(&v),
            Err(e) => { eprintln!("read {ino}: {e}"); reply.error(libc::EIO); }
        }
    }
    fn readdir(&mut self, _req: &Request, ino: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory) {
        let Some(Node::Dir { children }) = self.nodes.get(&ino) else { reply.error(libc::ENOTDIR); return };
        let padre = self.padres.get(&ino).copied().unwrap_or(1);
        let mut entries: Vec<(u64, FileType, String)> = vec![
            (ino, FileType::Directory, ".".into()),
            (padre, FileType::Directory, "..".into()),
        ];
        for (nombre, &cino) in children {
            let ft = match self.nodes.get(&cino) { Some(Node::Dir { .. }) => FileType::Directory, _ => FileType::RegularFile };
            entries.push((cino, ft, nombre.clone()));
        }
        for (i, (cino, ft, nombre)) in entries.into_iter().enumerate().skip(offset.max(0) as usize) {
            if reply.add(cino, (i + 1) as i64, ft, nombre) { break; }
        }
        reply.ok();
    }
}

pub fn montar(arch: &Path, punto: &Path) -> anyhow::Result<()> {
    let fs = VcFs::new(arch)?;
    println!("montado {} en {} (solo lectura; Ctrl-C o fusermount3 -u para desmontar)", arch.display(), punto.display());
    fuser::mount2(fs, punto, &[MountOption::RO, MountOption::FSName("vcompresor".into())])?;
    Ok(())
}
