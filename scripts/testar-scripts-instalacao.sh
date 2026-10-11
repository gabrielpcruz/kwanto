#!/bin/sh
# Roda os scripts de instalação (deb, rpm e AUR) com systemctl, udevadm e systemd-sysusers falsos
# e confere o que cada cenário chama, e em que ordem. Não precisa de root nem de systemd.
set -eu
cd "$(dirname "$0")/.."
raiz=$(pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/bin"
for cmd in systemctl udevadm systemd-sysusers; do
  printf '#!/bin/sh\necho "%s $*" >> "$LOG"\n' "$cmd" > "$tmp/bin/$cmd"
  chmod +x "$tmp/bin/$cmd"
done
export PATH="$tmp/bin:$PATH"
ESTADO="$tmp/estado"
falhas=0

# cenario <nome> <esperado (linhas separadas por ;)> <comando...>
cenario() {
  nome="$1"; esperado="$2"; shift 2
  LOG="$tmp/log"; export LOG; : > "$LOG"
  "$@"
  obtido=$(paste -sd';' "$LOG")
  if [ "$obtido" = "$esperado" ]; then
    echo "ok    $nome"
  else
    echo "FALHA $nome"; echo "  esperado: $esperado"; echo "  obtido:   $obtido"; falhas=$((falhas + 1))
  fi
}

PREP="systemd-sysusers kwanto.conf;udevadm control --reload;udevadm trigger --subsystem-match=powercap --action=add;systemctl daemon-reload"
# Na atualização, serviços que surgiram depois (kwanto-standby) são ligados se o kwantod estiver ligado.
LIGA_NOVO="systemctl is-enabled --quiet kwantod.service;systemctl enable --now kwanto-standby.service"
# Cópias dos scripts com o diretório de estado trocado pelo temporário (o original é fixo, pois roda
# como root e não deve aceitar caminho vindo do ambiente).
P="$tmp/scripts"; mkdir -p "$P" "$ESTADO"
for s in "$raiz"/packaging/scripts/*.sh; do
  sed "s|/var/lib/kwanto|$ESTADO|g" "$s" > "$P/$(basename "$s")"
done

cenario "deb: primeira instalação liga o serviço" "$PREP;systemctl enable --now kwanto-standby.service kwantod.service" sh "$P/pos-instalacao.sh" configure ""
cenario "deb: atualização só reinicia" "$PREP;$LIGA_NOVO;systemctl try-restart kwanto-standby.service kwantod.service" sh "$P/pos-instalacao.sh" configure 0.1.0
cenario "deb: prerm de atualização não desliga" "" sh "$P/antes-remover.sh" upgrade 0.2.0
cenario "deb: remoção desliga" "systemctl disable --now kwantod.service kwanto-standby.service" sh "$P/antes-remover.sh" remove
cenario "scripts não aceitam caminho do ambiente" "" sh -c "! grep -n KWANTO_ '$raiz'/packaging/scripts/*.sh"
cenario "deb: reinstalar depois de remover liga de novo" "$PREP;systemctl enable --now kwanto-standby.service kwantod.service" sh "$P/pos-instalacao.sh" configure 0.1.0
cenario "deb: atualização seguinte volta a só reiniciar" "$PREP;$LIGA_NOVO;systemctl try-restart kwanto-standby.service kwantod.service" sh "$P/pos-instalacao.sh" configure 0.2.0
cenario "rpm: instalação liga o serviço" "$PREP;systemctl enable --now kwanto-standby.service kwantod.service" sh "$P/pos-instalacao.sh" 1
cenario "rpm: atualização só reinicia" "$PREP;$LIGA_NOVO;systemctl try-restart kwanto-standby.service kwantod.service" sh "$P/pos-instalacao.sh" 2
cenario "rpm: preun de atualização não desliga" "" sh "$P/antes-remover.sh" 1
cenario "rpm: remoção desliga" "systemctl disable --now kwantod.service kwanto-standby.service" sh "$P/antes-remover.sh" 0

aur() { sh -c ". '$raiz/packaging/aur/kwanto.install' && $1"; }
cenario "aur: instalação cria o usuário antes da regra udev e do serviço" "$PREP;systemctl enable --now kwanto-standby.service kwantod.service" aur post_install
cenario "aur: atualização reaplica a regra e reinicia" "$PREP;$LIGA_NOVO;systemctl try-restart kwanto-standby.service kwantod.service" aur post_upgrade
cenario "aur: remoção desliga" "systemctl disable --now kwantod.service kwanto-standby.service" aur pre_remove

[ "$falhas" -eq 0 ] && echo "scripts de instalação ok" || { echo "$falhas cenário(s) falharam"; exit 1; }
