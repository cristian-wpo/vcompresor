cd "$(dirname "$0")/.."
mkdir -p bench/registros/fase3
exec bench/mide.sh -H vdeflate -e "F17 fase 3: las dos cotas" -m deflate-banco \
  -c "cota plana vs cota entropica (esquema de preflate)" \
  -n "el 13,14% era una cota ALTA de un modelo de tarifa plana, no una medida de la fase 3" \
  -- ./target/release/vc-fase3 /tmp/deflate-banco
