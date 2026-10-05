import Cocoa
import FinderSync

/// La extensión de Finder. Es lo único que pone una entrada en el NIVEL DE ARRIBA del menú
/// contextual, junto a «Comprimir» — un Service, por bien declarado que esté, va al final o
/// dentro de «Servicios», y en algunos Mac directamente no aparece.
///
/// EL PRECIO, y hay que decirlo: una extensión de Finder sólo actúa sobre los directorios que
/// declara vigilar. Aquí se declara la carpeta del usuario y los volúmenes montados, que cubre
/// donde la gente guarda cosas. Fuera de ahí no sale, y eso no es un fallo sino cómo funciona.
class Extension: FIFinderSync {

    override init() {
        super.init()
        var dirs: Set<URL> = [URL(fileURLWithPath: NSHomeDirectory())]
        for v in FileManager.default.mountedVolumeURLs(includingResourceValuesForKeys: nil,
                                                       options: [.skipHiddenVolumes]) ?? [] {
            dirs.insert(v)
        }
        FIFinderSyncController.default().directoryURLs = dirs
    }

    override func menu(for kind: FIMenuKind) -> NSMenu {
        let m = NSMenu(title: "")
        guard kind == .contextualMenuForItems || kind == .contextualMenuForContainer else { return m }
        let sel = FIFinderSyncController.default().selectedItemURLs() ?? []
        // Sobre un .vc lo que uno quiere es ABRIRLO, no meterlo en otro archivo. Ofrecer las dos
        // cosas siempre seria ruido; ofrecer la equivocada, peor.
        if sel.count == 1, sel[0].pathExtension.lowercased() == "vc" {
            m.addItem(withTitle: "Abrir con vcompresor", action: #selector(abrir(_:)), keyEquivalent: "")
        } else if !sel.isEmpty {
            m.addItem(withTitle: "Guardar con vcompresor", action: #selector(guardar(_:)), keyEquivalent: "")
        }
        for i in m.items { i.target = self }
        return m
    }

    /// Abre la app con lo seleccionado. La extensión NO comprime: vive dentro del Finder y un
    /// trabajo de horas ahí dentro bloquearía cosas que no son suyas.
    private func lanza(_ urls: [URL]) {
        guard !urls.isEmpty,
              let app = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.verticelabs.vcompresor")
        else { return }
        let cfg = NSWorkspace.OpenConfiguration()
        cfg.activates = true
        NSWorkspace.shared.open(urls, withApplicationAt: app, configuration: cfg)
    }

    @objc func guardar(_ s: AnyObject?) { lanza(FIFinderSyncController.default().selectedItemURLs() ?? []) }
    @objc func abrir(_ s: AnyObject?)   { lanza(FIFinderSyncController.default().selectedItemURLs() ?? []) }
}
