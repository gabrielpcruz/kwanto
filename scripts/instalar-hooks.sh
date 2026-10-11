#!/bin/sh
# Instala o hook de push. Ele só deixa enviar commits que passaram por scripts/verificar-limpo.sh (a
# validação completa num ambiente limpo, igual ao CI) e roda de novo a barreira de dados privados,
# que usa os identificadores desta máquina.
set -eu
cd "$(dirname "$0")/.."
hook=$(git rev-parse --git-path hooks)/pre-push
cat > "$hook" <<'HOOK'
#!/bin/sh
raiz=$(git rev-parse --show-toplevel)
verificado=$(cat "$(git rev-parse --git-dir)/kwanto-verificado" 2>/dev/null || true)
while read -r _ local_sha _ _; do
  case "$local_sha" in 0000000000000000000000000000000000000000) continue ;; esac
  if [ "$local_sha" != "$verificado" ]; then
    echo "push recusado: o commit $local_sha não passou pela verificação limpa." >&2
    echo "rode: sh scripts/verificar-limpo.sh" >&2
    exit 1
  fi
done
PATH="$HOME/.local/bin:$PATH" exec sh "$raiz/scripts/auditar-publicacao.sh"
HOOK
chmod +x "$hook"
echo "hook instalado: $hook"
