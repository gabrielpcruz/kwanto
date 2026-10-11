#!/bin/sh
# Só na remoção (deb: "remove"; rpm: 0), não na atualização.
set -e
ESTADO=/var/lib/kwanto
if [ "$1" = "remove" ] || [ "$1" = "0" ]; then
  systemctl disable --now kwantod.service kwanto-standby.service || true
  # No deb, reinstalar depois de remover chega como atualização ("configure <versão>"); o
  # marcador faz o pós-instalação ligar o serviço de novo.
  mkdir -p "$ESTADO" && touch "$ESTADO/.removido" || true
fi
