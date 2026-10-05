#!/usr/bin/env bash
# Arma el .app. Mete el binario DENTRO del bundle para que la app sea autocontenida: si
# dependiera de un vcompresor instalado aparte, la primera vez que alguien la abriera en otro
# Mac se encontraria un error en vez de un programa.
set -eu
A="$(cd "$(dirname "$0")" && pwd)"
MOTOR="${1:-$(dirname "$0")/../target/release/vcompresor}"
[ -x "$MOTOR" ] || { echo "no encuentro el motor en $MOTOR"; exit 1; }

swift build -c release --package-path "$A"
APP="$A/vcompresor.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$A/.build/release/VCompresor" "$APP/Contents/MacOS/vcompresor-app"
cp "$MOTOR" "$APP/Contents/Resources/vcompresor"
[ -f "$A/vcompresor.icns" ] && cp "$A/vcompresor.icns" "$APP/Contents/Resources/vcompresor.icns"
chmod +x "$APP/Contents/Resources/vcompresor"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>vcompresor</string>
  <key>CFBundleDisplayName</key><string>vcompresor</string>
  <key>CFBundleIdentifier</key><string>com.verticelabs.vcompresor</string>
  <key>CFBundleExecutable</key><string>vcompresor-app</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleIconFile</key><string>vcompresor</string>
  <!-- Sin esto la app arranca sin ventana ni menu: seria un proceso invisible. -->
  <key>LSUIElement</key><false/>
  <!-- Lo que sale al hacer clic derecho en el Finder. NSSendTypes con NSFilenamesPboardType
       es lo que hace que el sistema los ofrezca sobre ficheros y carpetas seleccionados. -->
  <!-- LO QUE SALE AL HACER CLIC DERECHO EN EL FINDER.
       La clave es NSSendFileTypes con UTIs: NSSendTypes con NSFilenamesPboardType es el nombre
       ANTIGUO del portapapeles y NO hace que el servicio aparezca sobre ficheros seleccionados.
       Con solo eso, el servicio se registra en el catalogo del sistema, responde si lo invocas
       a mano... y no sale nunca en el menu. Que es exactamente lo que paso. -->
  <key>NSServices</key><array>
    <dict>
      <key>NSMenuItem</key><dict><key>default</key><string>Guardar con vcompresor</string></dict>
      <key>NSMessage</key><string>comprimir</string>
      <key>NSPortName</key><string>vcompresor</string>
      <key>NSSendFileTypes</key><array><string>public.item</string></array>
      <key>NSSendTypes</key><array><string>public.file-url</string></array>
    </dict>
    <dict>
      <key>NSMenuItem</key><dict><key>default</key><string>Abrir con vcompresor</string></dict>
      <key>NSMessage</key><string>abrir</string>
      <key>NSPortName</key><string>vcompresor</string>
      <!-- Solo sobre .vc: su UTI se declara abajo. Ofrecer "abrir" sobre cualquier fichero
           seria ruido en el menu de todo el mundo. -->
      <key>NSSendFileTypes</key><array><string>com.verticelabs.vcompresor.archive</string></array>
      <key>NSSendTypes</key><array><string>public.file-url</string></array>
    </dict>
  </array>
  <!-- El tipo .vc, declarado. Ademas de servir al servicio de arriba, hace que el Finder
       llame a los .vc por su nombre en vez de "documento". -->
  <key>UTExportedTypeDeclarations</key><array><dict>
    <key>UTTypeIdentifier</key><string>com.verticelabs.vcompresor.archive</string>
    <key>UTTypeDescription</key><string>Archivo vcompresor</string>
    <key>UTTypeConformsTo</key><array><string>public.data</string><string>public.archive</string></array>
    <key>UTTypeTagSpecification</key><dict>
      <key>public.filename-extension</key><array><string>vc</string></array>
    </dict>
  </dict></array>
  <key>CFBundleDocumentTypes</key><array><dict>
    <key>CFBundleTypeName</key><string>Archivo vcompresor</string>
    <key>CFBundleTypeExtensions</key><array><string>vc</string></array>
    <key>CFBundleTypeRole</key><string>Editor</string>
  </dict></array>
</dict></plist>
PLIST

# --- LA EXTENSION DE FINDER ---------------------------------------------------------------
# Va DENTRO de la app, en Contents/PlugIns. Es lo unico que pone una entrada en el nivel de
# arriba del menu contextual; un Service, por bien declarado que este, acaba al final o dentro
# de «Servicios» — y en algunos Mac no aparece.
swift build -c release --product FinderExt --package-path "$A"
EXT="$APP/Contents/PlugIns/FinderExt.appex"
mkdir -p "$EXT/Contents/MacOS"
cp "$A/.build/release/FinderExt" "$EXT/Contents/MacOS/FinderExt"
cat > "$EXT/Contents/Info.plist" <<EXTPLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>vcompresor</string>
  <key>CFBundleDisplayName</key><string>vcompresor</string>
  <key>CFBundleIdentifier</key><string>com.verticelabs.vcompresor.finder</string>
  <key>CFBundleExecutable</key><string>FinderExt</string>
  <key>CFBundlePackageType</key><string>XPC!</string>
  <key>CFBundleShortVersionString</key><string>0.1</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <!-- macOS EXIGE esto en un .appex. Sin ello, pluginkit no da error: simplemente no registra
       la extension, y uno se queda mirando un menu que nunca cambia. -->
  <key>CFBundleSupportedPlatforms</key><array><string>MacOSX</string></array>
  <key>DTPlatformName</key><string>macosx</string>
  <key>DTSDKName</key><string>macosx</string>
  <key>NSExtension</key><dict>
    <key>NSExtensionPointIdentifier</key><string>com.apple.FinderSync</string>
    <key>NSExtensionPrincipalClass</key><string>FinderExt.Extension</string>
  </dict>
</dict></plist>
EXTPLIST
# La extension se firma ANTES que la app: firmar la app despues sella lo que hay dentro, y al
# reves la firma de fuera quedaria invalidada por lo de dentro.
codesign --force --sign - "$EXT" 2>/dev/null || echo "  aviso: no se pudo firmar la extension"

# Firma AD-HOC. Sin firmar, macOS mata la app al abrirla en un Mac con Apple Silicon.
# Ad-hoc basta para usarla uno mismo; para repartirla hace falta una cuenta de desarrollador
# y notarizacion, y eso es una decision del dueno, no algo que se hace de tapadillo.
codesign --force --deep --sign - "$APP" 2>/dev/null || echo "  aviso: no se pudo firmar"
echo "listo: $APP"
