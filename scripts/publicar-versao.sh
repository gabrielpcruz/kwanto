#!/bin/sh
# Uso: scripts/publicar-versao.sh v0.2.0
# Compila os pacotes a partir da tag (container Ubuntu 22.04), gera e assina o manifesto (pede a
# senha da chave), confere tudo com a chave pública do repositório e só então envia ao rascunho
# do release criado pelo CI e publica. Nada que o CI produziu é assinado.
set -eu
cd "$(dirname "$0")/.."
# Uma chave de teste definida no ambiente iria parar dentro dos binários oficiais.
if [ -n "${KWANTO_CHAVE_PUBLICA:-}" ]; then
  echo "KWANTO_CHAVE_PUBLICA está definida (chave de teste?). Rode: unset KWANTO_CHAVE_PUBLICA" >&2
  exit 1
fi
tag="${1:?informe a tag, por exemplo v0.2.0}"
versao="${tag#v}"
git rev-parse -q --verify "refs/tags/$tag" >/dev/null || { echo "a tag $tag não existe aqui" >&2; exit 1; }
gh release view "$tag" --json isDraft --jq .isDraft | grep -qx true \
  || { echo "o release $tag precisa existir como rascunho (o CI cria ao receber a tag)" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'git worktree remove --force "$tmp/fonte" 2>/dev/null; rm -rf "$tmp"' EXIT
git worktree add -q --detach "$tmp/fonte" "$tag"
cd "$tmp/fonte"
sh scripts/gerar-pacotes.sh
cargo run -q -p kwanto-release -- gerar-manifesto "$versao" dist
cargo run -q -p kwanto-release -- assinar dist/manifesto.json
# Confere com a chave pública que está na própria tag, como as bandejas vão conferir.
cargo run -q -p kwanto-release -- conferir "$versao" dist
# E os binários dentro do pacote: só podem confiar na chave oficial.
binarios="$tmp/binarios"; mkdir -p "$binarios"
dpkg-deb --fsys-tarfile dist/*.deb | tar -xO ./usr/libexec/kwanto/kwanto-atualizar > "$binarios/kwanto-atualizar"
dpkg-deb --fsys-tarfile dist/*.deb | tar -xO ./usr/bin/kwanto > "$binarios/kwanto"
cargo run -q -p kwanto-release -- conferir-binarios "$binarios/kwanto-atualizar" "$binarios/kwanto"

gh release upload "$tag" dist/*.deb dist/*.rpm dist/manifesto.json dist/manifesto.json.minisig --clobber
gh release edit "$tag" --draft=false --latest
echo "Kwanto $versao publicado."
