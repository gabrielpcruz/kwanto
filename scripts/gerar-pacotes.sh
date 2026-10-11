#!/bin/sh
# Compila em modo release e gera dist/*.deb e dist/*.rpm (exige nfpm: scripts/instalar-nfpm.sh).
# Fora do Ubuntu 22.04, compila num container 22.04 (docker) para os binários não exigirem uma
# glibc mais nova que a das distribuições suportadas.
set -eu
cd "$(dirname "$0")/.."
KWANTO_VERSAO=$(cargo metadata --no-deps --format-version 1 \
  | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "kwanto"))')
export KWANTO_VERSAO
COMPILAR="cargo build --release --locked -p kwantod -p kwanto -p kwanto-atualizar -p kwanto-standby"
if grep -q '^VERSION_ID="22.04"' /etc/os-release 2>/dev/null; then
  $COMPILAR
else
  docker build -q -t kwanto-compilacao -f packaging/compilacao.Dockerfile packaging >/dev/null
  # Saída em target/ubuntu2204 (não mistura com a compilação local) e cache do cargo reaproveitado.
  mkdir -p "${HOME}/.cache/kwanto-cargo"
  docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp -e CARGO_HOME=/cargo \
    -e CARGO_TARGET_DIR=/src/target/ubuntu2204 ${KWANTO_CHAVE_PUBLICA:+-e KWANTO_CHAVE_PUBLICA} \
    -v "$PWD:/src" -v "${HOME}/.cache/kwanto-cargo:/cargo" kwanto-compilacao $COMPILAR
  mkdir -p target/release
  cp target/ubuntu2204/release/kwantod target/ubuntu2204/release/kwanto \
     target/ubuntu2204/release/kwanto-atualizar target/ubuntu2204/release/kwanto-standby target/release/
fi
rm -rf dist
mkdir -p dist
nfpm pkg --config packaging/nfpm.yaml --packager deb --target "dist/kwanto_${KWANTO_VERSAO}_amd64.deb"
nfpm pkg --config packaging/nfpm.yaml --packager rpm --target "dist/kwanto-${KWANTO_VERSAO}-1.x86_64.rpm"
ls -l dist/
