#!/bin/sh
# Validação completa do projeto, na ordem certa. É exatamente o que o CI roda (ele só chama este
# script), então o que passa aqui passa lá. Instala em ~/.local/bin as ferramentas fixadas que faltarem.
# Para conferir num ambiente limpo, igual ao do CI: scripts/verificar-limpo.sh.
set -eu
cd "$(dirname "$0")/.."
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
passo() { printf '\n==> %s\n' "$*"; }

# Ferramenta na versão exata exigida, ou instala.
garantir() { # garantir <comando> <versão> <script de instalação>
  if ! "$1" --version 2>/dev/null | grep -qF "$2" && ! "$1" version 2>/dev/null | grep -qF "$2"; then
    sh "scripts/$3"
  fi
}

passo "ferramentas"
garantir gitleaks 8.30.1 instalar-gitleaks.sh
garantir cargo-deny 0.20.2 instalar-cargo-deny.sh
command -v google-chrome >/dev/null || command -v chromium >/dev/null \
  || { echo "falta o Chrome (usado no teste de XSS do painel)"; exit 1; }

passo "formatação";                 cargo fmt --all --check
passo "clippy";                     cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
passo "testes";                     cargo test --workspace --all-features --locked
passo "scripts de instalação";      sh scripts/testar-scripts-instalacao.sh
passo "unidades systemd";           sh scripts/testar-unidades.sh
passo "barreira (casos de teste)";  sh scripts/testar-auditoria.sh
passo "painel no navegador (XSS)";  sh scripts/testar-web.sh
passo "dependências";               cargo deny check
passo "barreira de publicação";     sh scripts/auditar-publicacao.sh

printf '\nverificação completa: ok\n'
