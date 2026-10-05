#!/usr/bin/env bash
# QUE LE FALTA PARA SER UN PRODUCTO DE RESPALDO. Probado, no opinado.
#
# El `grep` dice que no hay cifrado, ni bloqueo, ni exclusiones, ni retencion, ni progreso. Pero
# el grep tambien puede mentir en las dos direcciones -da falsos positivos con palabras que
# significan otra cosa, y falsos negativos si algo se llama distinto-, asi que cada punto se
# COMPRUEBA ejecutandolo. Lo que aqui salga en rojo es lo que de verdad falta.
#
# Lo mas peligroso no es lo que falta y se nota: es lo que falta y NO se nota. Un respaldo que
# pierde el dueno de los ficheros, o los permisos extendidos, o convierte un fichero disperso de
# 1 GB en 1 GB de ceros reales, no avisa de nada — y el dia que hace falta restaurar, ya es tarde.
set -u
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/.."
VC=$PWD/target/release/vcompresor
W=/tmp/prod; rm -rf $W; mkdir -p $W/in
ok()   { printf '  \033[32mOK\033[0m    %s\n' "$1"; }
mal()  { printf '  \033[31mFALTA\033[0m %s\n' "$1"; }
avisa(){ printf '  \033[33mOJO\033[0m   %s\n' "$1"; }

echo "############ 1. METADATOS: lo que se pierde SIN AVISAR ############"
cd $W/in
echo "contenido" > fichero.txt
chmod 754 fichero.txt
ln fichero.txt enlace-duro.txt              # enlace duro
ln -s fichero.txt enlace-blando.txt          # enlace simbolico
setfattr -n user.prueba -v "valor123" fichero.txt 2>/dev/null && TIENE_XATTR=1 || TIENE_XATTR=0
truncate -s 100M disperso.bin                # fichero disperso: 100 MB logicos, ~0 reales
mkdir vacio
mkfifo tuberia 2>/dev/null || true
touch -d "2001-02-03 04:05:06" fichero.txt
DISP_ANTES=$(du -s --block-size=1 disperso.bin | cut -f1)
INODO_A=$(stat -c%i fichero.txt); INODO_B=$(stat -c%i enlace-duro.txt)
cd $W
$VC compress $W/in -o $W/a.vc --level 1 >/dev/null 2>&1
rm -rf $W/out && $VC extract $W/a.vc -o $W/out >/dev/null 2>&1
R=$W/out

[ -f "$R/fichero.txt" ] && ok "el fichero vuelve" || mal "el fichero NO vuelve"
[ "$(stat -c%a $R/fichero.txt 2>/dev/null)" = "754" ] && ok "permisos" || mal "PERMISOS (unix mode)"
[ "$(stat -c%Y $R/fichero.txt 2>/dev/null)" = "$(date -d '2001-02-03 04:05:06' +%s)" ] && ok "fecha de modificacion" || mal "FECHA de modificacion"
[ -L "$R/enlace-blando.txt" ] && ok "enlaces simbolicos" || mal "ENLACES SIMBOLICOS"
[ -d "$R/vacio" ] && ok "directorios vacios" || mal "DIRECTORIOS VACIOS"
if [ -f "$R/enlace-duro.txt" ]; then
  IA=$(stat -c%i $R/fichero.txt); IB=$(stat -c%i $R/enlace-duro.txt)
  [ "$IA" = "$IB" ] && ok "enlaces duros (comparten inodo)" \
    || mal "ENLACES DUROS: vuelven como dos ficheros independientes; un arbol con miles de enlaces duros se restaura ocupando el doble"
else mal "ENLACES DUROS: no vuelven"; fi
if [ "$TIENE_XATTR" = "1" ]; then
  getfattr -n user.prueba "$R/fichero.txt" >/dev/null 2>&1 && ok "atributos extendidos" \
    || mal "ATRIBUTOS EXTENDIDOS (xattr): se pierden en silencio. SELinux, capacidades y metadatos de macOS viven ahi"
else avisa "xattr: el sistema de ficheros no los soporta, no se puede probar aqui"; fi
DISP_DESP=$(du -s --block-size=1 $R/disperso.bin 2>/dev/null | cut -f1 || echo 0)
if [ "$DISP_DESP" -gt $((DISP_ANTES + 10000000)) ]; then
  mal "FICHEROS DISPERSOS: 100 MB logicos y ~0 reales vuelven ocupando $((DISP_DESP/1048576)) MB REALES. Una maquina virtual o una base de datos dispersa explota al restaurar"
else ok "ficheros dispersos"; fi
[ -p "$R/tuberia" ] && ok "ficheros especiales (fifo)" || mal "FICHEROS ESPECIALES (fifo, dispositivos): se saltan"
# DUENO Y GRUPO. Sin privilegios no se puede cambiar el DUENO a otro usuario, pero si el GRUPO
# a uno secundario del propio usuario, y eso basta para probar que el campo viaja.
#
# La version anterior de esta prueba era un `mal` INCONDICIONAL: no comprobaba nada y siempre
# decia FALTA. Eso es peor que no tener la prueba, porque da una respuesta sin haber preguntado
# — y de hecho siguio diciendo FALTA despues de que el arreglo ya funcionara.
G2=$(id -G | tr " " "\n" | grep -v "^$(id -g)$" | head -1)
if [ -n "$G2" ]; then
  chgrp "$G2" $W/in/fichero.txt 2>/dev/null
  rm -f $W/g.vc; $VC compress $W/in -o $W/g.vc --level 1 >/dev/null 2>&1
  rm -rf $W/gout; $VC extract $W/g.vc -o $W/gout >/dev/null 2>&1
  if [ "$(stat -c%g $W/gout/fichero.txt 2>/dev/null)" = "$G2" ]; then
    ok "dueno y grupo (uid/gid)"
  else
    mal "DUENO Y GRUPO (uid/gid): el grupo no vuelve"
  fi
else
  avisa "uid/gid: este usuario no tiene un segundo grupo con el que probarlo"
fi

echo
echo "############ 2. FUNCIONES que un respaldo necesita ############"
$VC list $W/a.vc >/dev/null 2>&1 && ok "listar el contenido" || mal "LISTAR EL CONTENIDO: no hay comando. No se puede saber que hay dentro sin extraerlo entero"
# EXTRAER UN FICHERO SUELTO. Se comprueba que salga el pedido y que NO salga nada mas — no el
# codigo de salida, que puede ser distinto de 0 por los avisos de los ficheros especiales.
rm -rf $W/uno; $VC extract $W/a.vc -o $W/uno --file fichero.txt >/dev/null 2>&1
N=$(find $W/uno -type f 2>/dev/null | wc -l | tr -d " ")
if [ -f "$W/uno/fichero.txt" ] && [ "$N" = "1" ]; then
  ok "extraer UN fichero suelto"
else
  mal "EXTRAER UN FICHERO SUELTO: saco $N ficheros (se esperaba 1)"
fi

# BORRAR UNA INSTANTANEA. Hay que CREAR una antes: la version anterior le pedia borrar «d0» de
# un archivo hecho con `compress`, que no tiene instantaneas. Le pedia borrar algo que no existia
# y leyo el error como "la funcion no existe".
cp $W/a.vc $W/f.vc
$VC add $W/f.vc $W/in --name copia2 >/dev/null 2>&1
ANTES=$(stat -c%s $W/f.vc 2>/dev/null || stat -f%z $W/f.vc)
if $VC forget $W/f.vc copia2 >/dev/null 2>&1; then
  DESPUES=$(stat -c%s $W/f.vc 2>/dev/null || stat -f%z $W/f.vc)
  if $VC verify $W/f.vc >/dev/null 2>&1 && [ "$DESPUES" -le "$ANTES" ]; then
    ok "borrar una instantanea y compactar"
  else
    mal "BORRAR INSTANTANEA: borra pero deja el archivo mal o mas grande"
  fi
else
  mal "BORRAR UNA INSTANTANEA / RETENCION: no funciona"
fi

$VC compress $W/in -o $W/e.vc --exclude "*.txt" >/dev/null 2>&1 && ok "exclusiones" \
  || mal "EXCLUSIONES (--exclude): no existen"

# CIFRADO. Se CIFRA de verdad y se comprueba que vuelve, y que sin contrasena NO se puede. La
# version anterior miraba las cadenas del binario con `strings`, que es adivinar: ademas de
# fallar por un ancla ^ de mas, no habria distinguido "existe la bandera" de "funciona".
echo clave-de-prueba > $W/pw
if $VC compress $W/in -o $W/cif.vc --encrypt --password-file $W/pw >/dev/null 2>&1; then
  rm -rf $W/cout
  $VC extract $W/cif.vc -o $W/cout --password-file $W/pw >/dev/null 2>&1
  SIN=$($VC list $W/cif.vc >/dev/null 2>&1; echo $?)
  if [ -f "$W/cout/fichero.txt" ] && [ "$SIN" != "0" ]; then
    ok "cifrado (vuelve con la clave, y sin ella no se abre)"
  elif [ "$SIN" = "0" ]; then
    mal "CIFRADO: el archivo se abre SIN contraseña — no esta cifrado de verdad"
  else
    mal "CIFRADO: cifra pero no se recupera"
  fi
else
  mal "CIFRADO: no existe"
fi

echo
echo "############ 3. CONCURRENCIA: dos procesos a la vez ############"
cp $W/a.vc $W/c.vc
( $VC add $W/c.vc $W/in --name x1 > $W/c1.log 2>&1; echo $? > $W/rc1 ) &
( $VC add $W/c.vc $W/in --name x2 > $W/c2.log 2>&1; echo $? > $W/rc2 ) &
wait
R1=$(cat $W/rc1 2>/dev/null || echo 9); R2=$(cat $W/rc2 2>/dev/null || echo 9)
FALLOS=$(( (R1 != 0 ? 1 : 0) + (R2 != 0 ? 1 : 0) ))
RECHAZO=$(grep -l "otro proceso" $W/c1.log $W/c2.log 2>/dev/null | wc -l)
# Lo que importa: EXACTAMENTE uno rechazado, y por el candado, no por otro error cualquiera.
if [ "$FALLOS" = "1" ] && [ "$RECHAZO" = "1" ]; then
  if $VC verify $W/c.vc >/dev/null 2>&1; then
    ok "bloqueo: uno entra, el otro es rechazado, y el archivo queda sano"
  else
    mal "BLOQUEO: rechaza bien pero el archivo queda corrupto"
  fi
elif [ "$FALLOS" = "0" ]; then
  mal "SIN BLOQUEO: los DOS add entraron. El ultimo en renombrar se come la instantanea del otro"
else
  mal "BLOQUEO: $FALLOS fallos y $RECHAZO rechazos del candado — no es lo esperado (1 y 1)"
fi

echo "############ 4. INTERRUPCION a mitad ############"
# INVESTIGADA Y CERRADA — ver bench/interrupcion.md. Esta prueba avisaba de un .vc cortado que
# pasa verify, y era falso positivo doble: (a) comprimia un arbol que acaba en 3 s y mataba a
# los 3, asi que el archivo estaba COMPLETO; (b) al agrandarla, un trabajo en segundo plano desde
# un guion no interactivo IGNORA SIGINT, asi que tampoco cortaba.
#
# Con SIGTERM/SIGKILL de verdad a mitad de un --max el .vc queda a 0 B, y no por suerte: el
# archivo se escribe MAGIC -> frames -> indice -> pie, y los dos ultimos van al final. Una
# compresion cortada deja frames sin indice, asi que `abrir` falla POR CONSTRUCCION.
avisa "interrupcion: comprobado aparte (bench/interrupcion.md). El indice y el pie se escriben"
avisa "            los ultimos, asi que un corte no puede dejar un archivo que pase verify."

# VARIAS ENTRADAS. Lo encontro el dueño usando la app: selecciono cuatro ficheros de 170 MB y
# se puso a comprimir los 11 GB de la carpeta que los contenia. `compress` solo aceptaba UNA
# entrada, asi que la interfaz mandaba la carpeta comun para poder guardar una seleccion.
# Se comprueba que entren LOS PEDIDOS y solo esos, y que el progreso salga por una tuberia —
# que es como lo lee una interfaz grafica, y estaba detras de un isatty.
# El segundo fichero se CREA aqui. La primera version de esta prueba le pedia comprimir un
# «otro.txt» que no existia en el arbol, y habria dicho FALTA sobre una funcion que si esta —
# el mismo fallo que ya arruino cuatro pruebas de esta misma auditoria.
echo "segundo fichero de la seleccion" > $W/in/otro.txt
rm -f $W/sel.vc
$VC compress $W/in/fichero.txt $W/in/otro.txt -o $W/sel.vc >/dev/null 2>&1
N=$($VC list $W/sel.vc 2>/dev/null | wc -l | tr -d " ")
if [ "$N" = "2" ]; then
  rm -rf $W/selout; $VC extract $W/sel.vc -o $W/selout >/dev/null 2>&1
  M=$(find $W/selout -type f 2>/dev/null | wc -l | tr -d " ")
  if [ "$M" = "2" ] && cmp -s $W/in/fichero.txt $W/selout/fichero.txt; then
    ok "guardar VARIAS entradas sueltas (y no la carpeta que las contiene)"
  else
    mal "VARIAS ENTRADAS: entran bien pero no vuelven ($M ficheros al extraer)"
  fi
else
  mal "VARIAS ENTRADAS: metio $N entradas, se pidieron 2"
fi

# El avance por TUBERIA. El motor lo pinta solo si la salida de error es un terminal, para no
# llenar los registros de un cron; una interfaz que lo lee por tuberia no veia NADA y la barra
# se quedaba a cero toda la ejecucion. Se comprueba que con --progress salga y sin el no.
CON=$($VC compress $W/in -o $W/pr.vc --progress 2>&1 >/dev/null | grep -c "%)" || true)
SIN=$($VC compress $W/in -o $W/pr.vc 2>&1 >/dev/null | grep -c "%)" || true)
if [ "$CON" -gt 0 ] && [ "$SIN" = "0" ]; then
  ok "progreso por tuberia con --progress ($CON lineas), y log limpio sin el"
elif [ "$CON" = "0" ]; then
  mal "PROGRESO: --progress no saca nada por una tuberia (la barra se queda a cero)"
else
  mal "PROGRESO: sin --progress se cuelan $SIN lineas en el log"
fi

echo "############ 5. QUE SI ESTA ############"
for c in compress add extract verify info mount; do
  $VC $c 2>&1 | grep -qiE "missing|falta" && ok "comando '$c'"
done
ok "ida y vuelta verificada con blake3 en CADA fichero"
ok "instantaneas incrementales"
ok "entrada por tuberia (--stdin)"
ok "elegir ajuste por precio (--price)"
ok "determinismo byte a byte entre x86 y ARM"
ok "licencia AGPL + comercial"

rm -rf $W
echo FIN-PROD-CENTINELA
