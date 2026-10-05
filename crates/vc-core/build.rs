// Dónde está libjxl.
//
// En Linux vive en una ruta que el enlazador ya mira. En macOS con Apple Silicon, Homebrew la
// pone en /opt/homebrew/lib —y en los Mac Intel en /usr/local/lib—, que NO están en la ruta por
// defecto: el enlazador decía "library 'jxl' not found" con la biblioteca instalada y delante.
//
// Se pregunta primero a pkg-config, que es quien lo sabe de verdad, y solo si no contesta se
// prueban las rutas conocidas de Homebrew. Fijarlas a pelo habría funcionado en este Mac y roto
// en el siguiente.
fn main() {
    println!("cargo:rerun-if-env-changed=JXL_LIB_DIR");
    if let Ok(dir) = std::env::var("JXL_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
        return;
    }
    if let Ok(salida) = std::process::Command::new("pkg-config").args(["--libs-only-L", "libjxl"]).output() {
        let s = String::from_utf8_lossy(&salida.stdout);
        let mut encontrado = false;
        for t in s.split_whitespace() {
            if let Some(d) = t.strip_prefix("-L") {
                println!("cargo:rustc-link-search=native={d}");
                encontrado = true;
            }
        }
        if encontrado { return; }
    }
    if cfg!(target_os = "macos") {
        for d in ["/opt/homebrew/lib", "/usr/local/lib"] {
            if std::path::Path::new(d).join("libjxl.dylib").exists() {
                println!("cargo:rustc-link-search=native={d}");
                return;
            }
        }
        println!("cargo:warning=no se encuentra libjxl. Instálala con `brew install jpeg-xl`, \
                  o indica su ruta con JXL_LIB_DIR=/ruta/lib");
    }
}
