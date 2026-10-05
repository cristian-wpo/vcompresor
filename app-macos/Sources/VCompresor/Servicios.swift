import AppKit
import UniformTypeIdentifiers

/// Lo que aparece al hacer clic derecho en el Finder.
///
/// macOS lo llama "Services": la app los declara en su Info.plist y el sistema los enseña en el
/// menú contextual. Sin esto la app solo se podía usar abriéndola y arrastrando, que para
/// guardar una carpeta que ya tienes delante en el Finder es dar un rodeo absurdo.
///
/// POR QUE SERVICES Y NO UNA "ACCIÓN RÁPIDA" DE AUTOMATOR: la acción rápida es un paquete
/// aparte que hay que instalar en ~/Library/Services y mantener sincronizado con la app. Un
/// Service va DENTRO de la app: se registra al moverla a Aplicaciones y desaparece si la
/// borras, que es lo que uno espera.
final class Servicios: NSObject {

    /// Comprimir lo seleccionado. Sale el diálogo de guardar, como en cualquier app de Mac —
    /// escribir el .vc al lado sin preguntar sería decidir por el usuario dónde deja las cosas.
    @objc func comprimir(_ pb: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString>) {
        guard let urls = pb.readObjects(forClasses: [NSURL.self]) as? [URL], !urls.isEmpty else {
            error.pointee = "No he recibido ningún archivo."
            return
        }
        DispatchQueue.main.async {
            NSApp.activate(ignoringOtherApps: true)
            // LO SELECCIONADO, tal cual. Antes, con varias cosas se cogía su CARPETA COMÚN
            // porque el motor solo aceptaba una entrada: seleccionar cuatro vídeos de Descargas
            // ponía a guardar los 11 GB de Descargas enteros. El motor ya acepta varias, así que
            // aquí no hay nada que adivinar.
            Abierto.compartido.archivo = nil
            Abierto.compartido.aComprimir = urls
        }
    }

    /// Abrir un .vc para ver qué hay dentro y recuperar.
    @objc func abrir(_ pb: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString>) {
        guard let urls = pb.readObjects(forClasses: [NSURL.self]) as? [URL],
              let vc = urls.first(where: { $0.pathExtension.lowercased() == "vc" }) else {
            error.pointee = "Eso no es un archivo .vc."
            return
        }
        DispatchQueue.main.async {
            NSApp.activate(ignoringOtherApps: true)
            Abierto.compartido.archivo = vc
        }
    }
}
