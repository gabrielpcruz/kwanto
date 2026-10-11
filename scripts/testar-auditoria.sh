#!/bin/sh
# Testa a barreira (scripts/auditar-publicacao.sh): planta variações de identificadores FICTÍCIOS num
# repositório temporário e exige que cada uma seja pega; e que um repositório limpo passe.
set -eu
raiz=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
export KWANTO_AUDITORIA_SO_EXTRAS=1   # não lê os identificadores desta máquina: só os de teste
cat > "$tmp/privados.txt" <<'P'
serial:ZX9Q4LMN7TPRW2KD
serial:0000000000000000
id:t10.2020202020202020
mac:a1:b2:c3:d4:e5:f6
uuid:7d3f2a90-4c1e-4b8a-9f2e-1a2b3c4d5e6f
exato:198.51.100.23
texto:Rua das Palmeiras 742
P
export KWANTO_AUDITORIA_PRIVADOS="$tmp/privados.txt"
falhas=0
# Montada na hora para o próprio teste não conter algo com cara de chave secreta.
chave_falsa() { printf 'RW%s' "RTY0Iy$(head -c 153 /dev/zero | base64 -w0)"; }

caso() { # caso <descrição> <espera: pega|passa> <conteúdo do arquivo>
  r="$tmp/repo"; rm -rf "$r"; git init -q "$r"
  printf '%s\n' "$3" > "$r/arquivo.txt"
  git -C "$r" add -A && git -C "$r" -c user.name=t -c user.email=t@t commit -qm teste
  if KWANTO_AUDITORIA_REPO="$r" sh "$raiz/scripts/auditar-publicacao.sh" >"$tmp/saida" 2>&1; then obtido=passa; else obtido=pega; fi
  if [ "$obtido" = "$2" ]; then echo "ok    $1"; else echo "FALHA $1 (esperado: $2, obtido: $obtido)"; sed 's/^/      /' "$tmp/saida" | tail -5; falhas=$((falhas + 1)); fi
}

caso "repositório limpo passa" passa "nada aqui além de 127.0.0.1 e GPU-1"
caso "serial inteiro" pega "id = ZX9Q4LMN7TPRW2KD"
caso "serial em minúsculas" pega "zx9q4lmn7tprw2kd"
caso "serial com espaços" pega "ZX9Q 4LMN 7TPR W2KD"
caso "trecho do meio do serial" pega "LMN7TPRW"
caso "final do serial" pega "7TPRW2KD"
caso "MAC com dois-pontos" pega "A1:B2:C3:D4:E5:F6"
caso "MAC sem separador" pega "a1b2c3d4e5f6"
caso "MAC com hífens" pega "a1-b2-c3-d4-e5-f6"
caso "UUID sem hífens" pega "7d3f2a904c1e4b8a9f2e1a2b3c4d5e6f"
caso "IP exato" pega "servidor 198.51.100.23"
caso "texto privado" pega "mora na rua das palmeiras 742"
caso "chave secreta minisign" pega "untrusted comment: x
$(chave_falsa)"
caso "serial só de zeros não gera alarme" passa "0000000000000000 e 2020202020202020"
caso_ignorado() { # arquivo rastreado num caminho que o .gitignore manda ignorar
  r="$tmp/repo"; rm -rf "$r"; git init -q "$r"; mkdir -p "$r/interno"
  echo "/interno" > "$r/.gitignore"; echo nota > "$r/interno/nota.md"
  git -C "$r" add -A && git -C "$r" add -f interno/nota.md
  git -C "$r" -c user.name=t -c user.email=t@t commit -qm teste
  if KWANTO_AUDITORIA_REPO="$r" sh "$raiz/scripts/auditar-publicacao.sh" >"$tmp/saida" 2>&1; then
    echo "FALHA arquivo rastreado em caminho ignorado (esperado: pega)"; falhas=$((falhas + 1))
  else echo "ok    arquivo rastreado em caminho ignorado"; fi
}
caso_ignorado
caso "IP parecido não pega" passa "198.51.100.230 não, 98.51.100.23 não"

[ "$falhas" -eq 0 ] && echo "barreira ok" || { echo "$falhas caso(s) falharam"; exit 1; }
