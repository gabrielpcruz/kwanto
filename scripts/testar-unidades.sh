#!/bin/sh
# Confere as propriedades de segurança das unidades systemd do pacote.
set -eu
cd "$(dirname "$0")/.."
falhas=0
tem() { # tem <unidade> <linha exata>
  if grep -qxF "$2" "packaging/$1"; then echo "ok    $1: $2"; else echo "FALHA $1: falta '$2'"; falhas=$((falhas + 1)); fi
}
nao_tem() { # nao_tem <unidade> <regex>
  if grep -qE "$2" "packaging/$1"; then echo "FALHA $1: não pode ter '$2'"; falhas=$((falhas + 1)); else echo "ok    $1: sem '$2'"; fi
}
# O serviço que fala HTTP não tem privilégio nenhum nem acesso a disco.
nao_tem kwantod.service '^SupplementaryGroups='
nao_tem kwantod.service '^AmbientCapabilities='
tem kwantod.service 'CapabilityBoundingSet='
nao_tem kwantod.service 'block-'
tem kwantod.service 'DevicePolicy=closed'
tem kwantod.service 'NoNewPrivileges=yes'
tem kwantod.service 'User=kwanto'
tem kwantod.service 'LimitNOFILE=65536'
# O consultor de standby: único com acesso a disco, sem rede, usuário próprio.
tem kwanto-standby.service 'User=kwanto-standby'
tem kwanto-standby.service 'SupplementaryGroups=disk'
tem kwanto-standby.service 'AmbientCapabilities=CAP_SYS_RAWIO'
tem kwanto-standby.service 'CapabilityBoundingSet=CAP_SYS_RAWIO'
tem kwanto-standby.service 'PrivateNetwork=yes'
tem kwanto-standby.service 'RestrictAddressFamilies=AF_UNIX'
tem kwanto-standby.service 'IPAddressDeny=any'
tem kwanto-standby.service 'DevicePolicy=closed'
tem kwanto-standby.service 'DeviceAllow=block-sd r'
tem kwanto-standby.service 'NoNewPrivileges=yes'
tem kwanto-standby.service 'RuntimeDirectory=kwanto-standby'
tem kwanto-standby.service 'RuntimeDirectoryMode=0750'
tem kwanto-standby.service 'Group=kwanto'
tem kwanto-standby.service 'UMask=0027'
tem kwanto.sysusers 'u kwanto-standby -:kwanto "Kwanto (standby dos HDDs)" - -'
if command -v systemd-analyze >/dev/null; then
  for u in kwantod.service kwanto-standby.service; do
    saida=$(systemd-analyze verify "packaging/$u" 2>&1 | grep -F "packaging/$u" | grep -v "Command .* is not executable" || true)
    if [ -n "$saida" ]; then echo "FALHA $u: $saida"; falhas=$((falhas + 1)); else echo "ok    $u: systemd-analyze verify"; fi
  done
fi
[ "$falhas" -eq 0 ] && echo "unidades ok" || { echo "$falhas falha(s)"; exit 1; }
