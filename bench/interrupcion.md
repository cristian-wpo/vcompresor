banco: la "trampa del Ctrl-C" no existe — era mi prueba, que no cortaba nada

La auditoria avisaba: "tras Ctrl-C queda un .vc que PASA verify (inesperado)". Si fuera un
archivo integro con la mitad de los ficheros seria el peor fallo posible en un archivador:
parece un respaldo bueno y no lo es. Investigado, y es un FALSO POSITIVO doble, los dos de la
propia prueba:

 1. LA PRUEBA NO CORTABA NADA. Comprimia un arbol que al nivel 19 acaba en menos de 3 segundos,
    y mataba a los 3. El .vc que "pasaba verify" estaba COMPLETO porque la compresion habia
    terminado antes de la senal. Yo lei "archivo completo que verifica" como "archivo cortado
    que verifica".

 2. Y AL AGRANDARLA, seguia sin cortar: un trabajo lanzado EN SEGUNDO PLANO desde un guion no
    interactivo IGNORA SIGINT por norma POSIX. El proceso siguio vivo 137 s despues del kill.
    Eso tampoco es un fallo del programa: un Ctrl-C de verdad en un terminal va al grupo de
    procesos en primer plano y si lo mata.

MEDIDO con senales que no se ignoran, sobre 2.000 ficheros con --max (minutos de trabajo):
    SIGTERM a los 4 s -> el .vc queda a 0 B
    SIGKILL a los 4 s -> el .vc queda a 0 B
El archivo se escribe al final, asi que una compresion cortada deja un fichero vacio que no
engana a nadie. No hay trampa.

TRES PRUEBAS MIAS EN DOS DIAS QUE DABAN UNA RESPUESTA SIN HABER PREGUNTADO: la de uid/gid (un
fallo incondicional), la de concurrencia (comprobaba algo que pasa igual con candado y sin el) y
esta (no cortaba nada). Las tres dieron confianza falsa —dos en negativo y una en positivo— y
ninguna la habria detectado el compilador. Una prueba hay que verla FALLAR al menos una vez para
saber que puede.
