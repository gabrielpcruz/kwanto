#!/bin/sh
# Instala o nfpm na conta do usuário (~/.local/bin), conferindo o SHA-256. O CI usa a mesma versão.
set -eu
VERSAO=2.47.0
# SHA-256 fixado aqui (não lido do próprio release, que só protegeria o transporte).
SHA256=0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783
ARQUIVO="nfpm_${VERSAO}_Linux_x86_64.tar.gz"
destino="${HOME}/.local/bin"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/$ARQUIVO" "https://github.com/goreleaser/nfpm/releases/download/v${VERSAO}/$ARQUIVO"
echo "$SHA256  $tmp/$ARQUIVO" | sha256sum -c -
mkdir -p "$destino"
tar -xzf "$tmp/$ARQUIVO" -C "$destino" nfpm
"$destino/nfpm" --version | head -1
