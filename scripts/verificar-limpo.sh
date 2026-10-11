#!/bin/sh
# Roda scripts/verificar.sh num container Ubuntu 24.04 limpo, a partir de um clone do commit atual,
# como usuário comum (como no CI). Pega o que só funciona "na minha máquina": ferramenta instalada
# só aqui, arquivo sem commit, ordem errada de passos. Se passar, marca o commit como verificado; o
# hook de push só deixa enviar commits verificados.
set -eu
cd "$(dirname "$0")/.."
if [ -n "$(git status --porcelain)" ]; then
  echo "há mudanças sem commit: a verificação limpa usa só o que está no commit." >&2
  exit 1
fi
commit=$(git rev-parse HEAD)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
git clone -q --no-hardlinks "$PWD" "$tmp/fonte"
git -C "$tmp/fonte" checkout -q "$commit"
docker build -q -t kwanto-verificacao -f scripts/verificacao.Dockerfile scripts >/dev/null
cache="${HOME}/.cache/kwanto-verificacao"
mkdir -p "$cache/cargo" "$cache/alvo" "$cache/casa"
docker run --rm --user "$(id -u):$(id -g)" -e HOME=/casa -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/alvo \
  -v "$tmp/fonte:/src" -v "$cache/cargo:/cargo" -v "$cache/alvo:/alvo" -v "$cache/casa:/casa" \
  -w /src kwanto-verificacao sh scripts/verificar.sh
echo "$commit" > "$(git rev-parse --git-dir)/kwanto-verificado"
echo "commit $commit verificado em ambiente limpo"
