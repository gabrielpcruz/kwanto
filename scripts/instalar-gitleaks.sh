#!/bin/sh
# Instala o gitleaks (detector de segredos usado pela barreira de publicação) em ~/.local/bin,
# conferindo o SHA-256 fixado aqui.
set -eu
VERSAO=8.30.1
SHA256=551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb
ARQUIVO="gitleaks_${VERSAO}_linux_x64.tar.gz"
destino="${HOME}/.local/bin"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/$ARQUIVO" "https://github.com/gitleaks/gitleaks/releases/download/v${VERSAO}/$ARQUIVO"
echo "$SHA256  $tmp/$ARQUIVO" | sha256sum -c -
mkdir -p "$destino"
tar -xzf "$tmp/$ARQUIVO" -C "$destino" gitleaks
"$destino/gitleaks" version
