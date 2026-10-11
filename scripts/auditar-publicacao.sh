#!/bin/sh
# Barreira antes de publicar: procura em TODO o histórico (conteúdo, também binário, mensagens e
# autores) os identificadores reais desta máquina, dados privados e segredos. Roda no hook pre-push.
#
# Os identificadores são lidos na hora e nunca gravados no repositório. Dados privados extras vão em
# ~/.config/kwanto-release/privados.txt, um por linha (nome, endereço, e-mail pessoal, CPF, nº da
# instalação na distribuidora, serial da placa-mãe: `sudo cat /sys/class/dmi/id/board_serial`...).
# Linhas podem ter tipo: serial:, mac:, uuid:, id:, exato: (IPs), texto: (padrão).
set -eu
raiz=$(cd "$(dirname "$0")/.." && pwd)
repo="${KWANTO_AUDITORIA_REPO:-$raiz}"
privados="${KWANTO_AUDITORIA_PRIVADOS:-$HOME/.config/kwanto-release/privados.txt}"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
pad="$tmp/padroes"
: > "$pad"

com() { # com <tipo>: prefixa cada linha não vazia
  sed 's/^[[:space:]]*//; s/[[:space:]]*$//' | grep -v '^$' | sed "s/^/$1:/" >> "$pad" || true
}

if [ -z "${KWANTO_AUDITORIA_SO_EXTRAS:-}" ]; then
  # Discos e USB: seriais, WWID/EUI; o serial também é a parte final dos nomes em /dev/disk/by-id.
  cat /sys/block/*/device/serial /sys/class/nvme/*/serial /sys/bus/usb/devices/*/serial 2>/dev/null | com serial
  ls /dev/disk/by-id 2>/dev/null | sed -nE 's/-part[0-9]+$//; s/^[a-z]+-.*_([A-Za-z0-9-]{6,})$/\1/p' | com serial
  cat /sys/block/*/device/wwid /sys/block/*/wwid /sys/block/*/eui /sys/block/*/nguid 2>/dev/null | com id
  ls /dev/disk/by-id 2>/dev/null | sed -nE 's/^(wwn-0x|nvme-eui\.)([0-9a-f]+).*/\2/p' | com id
  { ls /dev/disk/by-uuid; ls /dev/disk/by-partuuid; } 2>/dev/null | com uuid
  ls /dev/disk/by-label 2>/dev/null | com texto
  # Máquina: machine-id, UUID das GPUs NVIDIA, nome.
  cat /etc/machine-id /var/lib/dbus/machine-id 2>/dev/null | com uuid
  nvidia-smi -L 2>/dev/null | sed -nE 's/.*UUID: GPU-([0-9a-f-]+).*/\1/p' | com uuid
  hostname 2>/dev/null | com texto
  # Rede: MACs (menos o zerado), IPs da rede local e nomes das conexões (Wi-Fi).
  cat /sys/class/net/*/address 2>/dev/null | grep -v '^00:00:00:00:00:00$' | com mac
  ip -o addr show 2>/dev/null | awk '{print $4}' | sed 's|/.*||' | grep -vE '^(127\.|::1$)' | com exato
  nmcli -t -f NAME connection show 2>/dev/null | grep -vxE 'lo|Wired connection [0-9]+|Conexão cabeada [0-9]+' | com texto
  # A chave privada de assinatura: nenhum trecho dela pode estar no repositório.
  [ -f "$HOME/.config/kwanto-release/kwanto.key" ] && sed -n 2p "$HOME/.config/kwanto-release/kwanto.key" | com chave
fi
[ -f "$privados" ] && grep -v '^[[:space:]]*#' "$privados" >> "$pad"

falhas=0
# Arquivo rastreado num caminho que o .gitignore manda ignorar (ex.: material interno adicionado à força).
if git -C "$repo" ls-files -ci --exclude-standard | grep -q .; then
  echo "ARQUIVOS RASTREADOS EM CAMINHO IGNORADO (não deveriam ser publicados):"
  git -C "$repo" ls-files -ci --exclude-standard | sed 's/^/  /'
  falhas=1
fi
python3 -I "$raiz/scripts/auditoria.py" "$repo" "$pad" || falhas=1

if command -v gitleaks >/dev/null; then
  if gitleaks git --no-banner --redact --log-opts="--all" "$repo" >"$tmp/gl" 2>&1 \
     && gitleaks dir --no-banner --redact "$repo" >>"$tmp/gl" 2>&1; then
    echo "ok: gitleaks não encontrou segredos (histórico e árvore)"
  else
    cat "$tmp/gl"; falhas=1
  fi
else
  echo "gitleaks não instalado; instale para publicar" >&2; falhas=1
fi

[ "$falhas" -eq 0 ] && echo "auditoria ok: pode publicar" || { echo "auditoria FALHOU: nada foi publicado" >&2; exit 1; }
