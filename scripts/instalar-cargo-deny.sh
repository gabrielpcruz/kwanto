#!/bin/sh
# Instala o cargo-deny (licenças e alertas de segurança das dependências) em ~/.local/bin,
# conferindo o SHA-256 fixado aqui.
set -eu
VERSAO=0.20.2
SHA256=9f12ed4c49936e09b48bf862b595cde2fe64fcbd9d74dfacac6131ca824c8d5f
NOME="cargo-deny-${VERSAO}-x86_64-unknown-linux-musl"
destino="${HOME}/.local/bin"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL -o "$tmp/$NOME.tar.gz" "https://github.com/EmbarkStudios/cargo-deny/releases/download/${VERSAO}/$NOME.tar.gz"
echo "$SHA256  $tmp/$NOME.tar.gz" | sha256sum -c -
mkdir -p "$destino"
tar -xzf "$tmp/$NOME.tar.gz" -C "$tmp"
install -m 755 "$tmp/$NOME/cargo-deny" "$destino/cargo-deny"
"$destino/cargo-deny" --version
