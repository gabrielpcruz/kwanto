#!/bin/sh
# Depois de instalar ou atualizar. deb: "configure" (+ versão anterior na atualização);
# rpm: 1 na instalação, 2 na atualização.
set -e
# Marcador gravado pelo antes-remover.sh: o pacote foi removido (sem purge) e agora volta.
ESTADO=/var/lib/kwanto
systemd-sysusers kwanto.conf || true
udevadm control --reload || true
udevadm trigger --subsystem-match=powercap --action=add || true
systemctl daemon-reload || true
if { [ "$1" = "configure" ] && [ -z "${2:-}" ]; } || [ "$1" = "1" ] || [ -e "$ESTADO/.removido" ]; then
  rm -f "$ESTADO/.removido"
  systemctl enable --now kwanto-standby.service kwantod.service || true
else
  # Atualização: mantém a escolha do usuário (ligado ou desligado). Um serviço que surgiu numa versão
  # nova (kwanto-standby) acompanha o kwantod.
  if systemctl is-enabled --quiet kwantod.service; then
    systemctl enable --now kwanto-standby.service || true
  fi
  systemctl try-restart kwanto-standby.service kwantod.service || true
fi
