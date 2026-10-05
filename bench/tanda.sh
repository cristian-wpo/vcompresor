#!/usr/bin/env bash
# LA TANDA LARGA: el coste por nivel y el control de cero, seguidos y sin tocar nada.
#
# Se hacen juntos porque comparten la unica condicion que los dos necesitan y que es facil de
# romper sin darse cuenta: QUE NADIE TOQUE EL REPOSITORIO MIENTRAS CORREN. La ultima vez el
# control salio con la vuelta 1 limpia y las vueltas 2 y 3 sucias, y el motivo fue que yo copie
# un guion nuevo al repo a mitad de la tanda. La guardia lo detecto; el tiempo se perdio igual.
#
# ~29 minutos. En SERIE. No abrir editores, no copiar ficheros, no generar el panel.
set -uo pipefail
cd "$(dirname "$0")/.."
echo "=================== TANDA LARGA · no tocar el repo hasta FIN-TANDA ==================="
echo
./bench/niveles.sh || exit $?
echo
./bench/control-cero.sh || exit $?
echo
echo "Ahora ya se puede tocar. Toca: python3 bench/dinero.py && python3 bench/panel.py"
echo FIN-TANDA
