#!/bin/sh
# Confere os pacotes de dist/ e instala em containers (Ubuntu 22.04/24.04, Debian 12 e Fedora), sem systemd.
set -eu
cd "$(dirname "$0")/.."
deb=$(ls dist/kwanto_*_amd64.deb)
rpm=$(ls dist/kwanto-*.x86_64.rpm)
ARQUIVOS="/usr/bin/kwantod /usr/bin/kwanto /usr/libexec/kwanto/kwanto-atualizar /usr/libexec/kwanto/kwanto-standby
/usr/lib/systemd/system/kwantod.service /usr/lib/systemd/system/kwanto-standby.service /usr/lib/sysusers.d/kwanto.conf
/usr/lib/udev/rules.d/60-kwanto.rules /usr/share/polkit-1/actions/io.github.gabrielpcruz.kwanto.policy
/usr/share/applications/kwanto.desktop /etc/xdg/autostart/kwanto.desktop
/usr/share/icons/hicolor/scalable/apps/kwanto.svg"

conteudo=$(dpkg-deb -c "$deb")
for f in $ARQUIVOS; do
  echo "$conteudo" | grep -q " \.$f\$" || { echo "faltou $f no .deb"; exit 1; }
done
dpkg-deb -I "$deb" | grep -q 'libwebkit2gtk-4.1-0' || { echo "dependência ausente no .deb"; exit 1; }
# O atualizador chama o pkexec; no Debian 12+ e no Ubuntu 23.04+ ele é um pacote à parte do polkitd.
dpkg-deb -I "$deb" | grep -q 'pkexec | policykit-1' || { echo "o .deb não garante o pkexec"; exit 1; }
echo "conteúdo do .deb ok"

for imagem in ubuntu:22.04 debian:12 ubuntu:24.04; do
docker run --rm -v "$PWD/dist:/dist:ro" "$imagem" sh -ec "
  apt-get update -qq >/dev/null
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq /dist/$(basename "$deb") >/dev/null
  id kwanto && id kwanto-standby
  test -x /usr/libexec/kwanto/kwanto-atualizar
  command -v pkexec >/dev/null
  dpkg-query -S /usr/bin/kwanto >/dev/null
  if ldd /usr/bin/kwantod /usr/bin/kwanto | grep 'not found'; then exit 1; fi
  echo 'deb ok em $imagem'"
done

docker run --rm -v "$PWD/dist:/dist:ro" fedora:latest sh -ec "
  for f in $(echo $ARQUIVOS); do rpm -qlp /dist/$(basename "$rpm") | grep -qx \"\$f\" || { echo \"faltou \$f no .rpm\"; exit 1; }; done
  dnf install -y -q /dist/$(basename "$rpm") >/dev/null
  id kwanto && id kwanto-standby
  test -x /usr/libexec/kwanto/kwanto-atualizar
  command -v pkexec >/dev/null
  rpm -qf /usr/bin/kwanto >/dev/null
  if ldd /usr/bin/kwantod /usr/bin/kwanto | grep 'not found'; then exit 1; fi
  echo 'rpm ok em fedora'"
