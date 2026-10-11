#!/bin/sh
# Gera as imagens do README com o Chrome headless e dados fictícios (nada da máquina real):
# assets/marca.png (banner) e assets/dashboard.png (painel). Exige Chrome.
set -eu
cd "$(dirname "$0")/.."
chrome=$(command -v google-chrome || command -v chromium || command -v chromium-browser) || { echo "sem Chrome"; exit 1; }
porta=18578
python3 -I scripts/teste-web/servidor.py web "$porta" demo &
servidor=$!
perfil=$(mktemp -d)
trap 'kill $servidor 2>/dev/null; rm -rf "$perfil"' EXIT
sleep 1
mkdir -p assets
capturar() { # capturar <caminho> <largura,altura> <saída>
  "$chrome" --headless=new --disable-gpu --no-sandbox --hide-scrollbars --user-data-dir="$perfil" \
    --force-device-scale-factor=2 --default-background-color=00000000 --virtual-time-budget=5000 \
    --window-size="$2" --screenshot="$PWD/$3" "http://127.0.0.1:$porta$1" >/dev/null 2>&1
  echo "gerado $3"
}
capturar /_marca 1282,362 assets/marca.png
capturar / 1280,860 assets/dashboard.png
