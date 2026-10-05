#!/usr/bin/env bash
# Trae las MUESTRAS de preflate-rs 0.7.6, que es lo unico que le faltaba a su suite para correr.
# Solo lo que las pruebas piden: los .deflate (los recorre `test_on_all_deflate_files`) y el
# sample1.bin.gz. Los JPEG/lepton de 14 MB no los toca ninguna prueba de las que fallan.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p crates/samples
python3 - <<'PY'
import json, os, urllib.request
url = "https://api.github.com/repos/microsoft/preflate-rs/contents/samples?ref=v0.7.6"
d = json.load(urllib.request.urlopen(url, timeout=30))
quiero = [x for x in d if x["name"].endswith(".deflate") or x["name"] == "sample1.bin.gz"]
total = 0
for x in quiero:
    destino = os.path.join("crates/samples", x["name"])
    if os.path.exists(destino) and os.path.getsize(destino) == x["size"]:
        total += x["size"]; continue
    with urllib.request.urlopen(x["download_url"], timeout=120) as r:
        b = r.read()
    assert len(b) == x["size"], f'{x["name"]}: {len(b)} != {x["size"]}'
    open(destino, "wb").write(b)
    total += len(b)
print(f'{len(quiero)} ficheros, {total/1e6:.1f} MB en crates/samples/')
PY
ls crates/samples | wc -l
