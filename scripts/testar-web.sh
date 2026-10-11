#!/bin/sh
# Teste do navegador: abre o dashboard e o popup reais (web/) com uma API falsa cheia de rótulos
# hostis e exige que nenhum script injetado rode (marcador data-xss no <body>). Exige Chrome.
set -eu
cd "$(dirname "$0")/.."
chrome=$(command -v google-chrome || command -v chromium || command -v chromium-browser) || { echo "sem Chrome"; exit 1; }
porta=18577
python3 -I scripts/teste-web/servidor.py web "$porta" &
servidor=$!
trap 'kill $servidor 2>/dev/null' EXIT
sleep 1
falhas=0
for pagina in / /mini; do
  dom=$("$chrome" --headless=new --disable-gpu --no-sandbox --user-data-dir="$(mktemp -d)" \
        --virtual-time-budget=4000 --dump-dom "http://127.0.0.1:$porta$pagina" 2>/dev/null)
  if printf '%s' "$dom" | grep -q 'data-xss'; then
    echo "FALHA $pagina: script injetado pelos dados rodou"; falhas=$((falhas + 1))
  elif ! printf '%s' "$dom" | grep -q 'GPU'; then
    echo "FALHA $pagina: a página não mostrou os dados (teste inválido)"; falhas=$((falhas + 1))
  else
    echo "ok    $pagina: dados hostis mostrados como texto"
  fi
done
[ "$falhas" -eq 0 ] && echo "web ok" || exit 1
