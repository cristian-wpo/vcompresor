#!/usr/bin/env bash
# ¿`fila` SOBRA EN EL PERFIL RAPIDO? La pregunta que el panel lleva marcando DESANCLADA.
#
# El racimo `fila` -los modelos 11,12,13,17,18,19: columna y encima- cuesta entre el 12% y el 17%
# del reloj y devuelve entre el 0,1% y el 4,3% de bytes. Eso no es un fallo que arreglar: es un
# INTERCAMBIO, y quien lo decide es el precio. Por eso la decision fue «al menu», no «fuera»: al
# perfil Rapido (perfil 2), donde el que lo elige ya ha dicho que prefiere reloj.
#
# 🔑 SE MIDE SIN TOCAR CODIGO. `apagados_de_perfil(perfil) | apaga_mascara()` componen por OR, asi
# que `VC_PERFIL=2 VC_APAGAMASK=931840` es exactamente «el perfil rapido, y ademas fila fuera».
# Compilar dos veces para saber si merece la pena compilar es el orden al reves.
#
#   931840 = (1<<11)|(1<<12)|(1<<13)|(1<<17)|(1<<18)|(1<<19)
#
# ⚠️ El perfil VIAJA en el archivo desde que se arreglo `perfil_a_escribir`, asi que estas dos
# configuraciones producen archivos que se abren solos. La mascara NO viaja —es de banco— y por
# eso esto es una MEDIDA, no una propuesta de defecto: lo que se aplique despues ira por perfil.
#
# EN SERIE. Si la V3 sigue corriendo, el cerrojo de mide.sh hace esperar a esta sin que nadie
# tenga que vigilar.
set -uo pipefail
cd "$(dirname "$0")/.."
S=/tmp/compresor-f1/samples
W=$(mktemp -d /tmp/v4.XXXXXX)
trap "rm -rf $W" EXIT
FILA=931840
for m in codigo.10m csvjson.10m enwik8.10m logs.10m sql.10m; do
  VC_PERFIL=2 bench/mide.sh -H vc-cm -e "V4 fila en el perfil rapido" -m "$m" \
    -c "perfil 2 (con fila)" \
    -n "Control: el perfil Rapido tal y como esta hoy. La pregunta es si fila deberia salir tambien de aqui." \
    -- ./target/release/vcompresor compress "$S/$m" -o "$W/a.vc" --max
  VC_PERFIL=2 VC_APAGAMASK=$FILA bench/mide.sh -H vc-cm -e "V4 fila en el perfil rapido" -m "$m" \
    -c "perfil 2 sin fila" \
    -n "El perfil Rapido con el racimo fila (11,12,13,17,18,19) tambien apagado." \
    -- ./target/release/vcompresor compress "$S/$m" -o "$W/b.vc" --max
done
echo "== V4 terminada =="
