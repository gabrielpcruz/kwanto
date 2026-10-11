"""Procura identificadores privados em todo o histórico de um repositório git.

Lê padrões "tipo:valor" (um por linha) do arquivo dado e varre o conteúdo de todos os objetos de
todos os commits (inclusive binários), as mensagens e os autores. Tipos:
  serial, id, uuid, mac  normalizados (só [0-9a-z]) e procurados por qualquer trecho de 8 caracteres
                         (10 se só hexadecimal), pegando cópias parciais, com espaços ou hífens
  exato                  procurado como está, sem dígito colado antes ou depois (IPs)
  texto                  normalizado e procurado inteiro (nomes, endereços, e-mails)
  chave                  normalizado e procurado por trechos de 12 caracteres (chave privada)
Sai com 1 se achar algo. Mostra só os 4 primeiros caracteres de cada valor.
"""
import re
import subprocess
import sys

repo, arquivo_padroes = sys.argv[1], sys.argv[2]


def norm(s):
    return re.sub(r"[^0-9a-z]", "", s.lower())


padroes = []  # (tipo, valor original, função de busca no texto normalizado/original)
for linha in open(arquivo_padroes, encoding="utf-8", errors="replace"):
    linha = linha.strip()
    if not linha:
        continue
    tipo, _, valor = linha.partition(":") if re.match(r"^(serial|id|uuid|mac|exato|texto|chave):", linha) else ("texto", "", linha)
    valor = valor.strip()
    if tipo == "id":
        # Prefixos de formato (naa., eui., nvme., t10.) são comuns a todos os discos.
        valor = re.sub(r"^(naa|eui|nvme|t10|wwn-0x|0x)\.?", "", valor, flags=re.I)
    n = norm(valor)
    if tipo == "exato":
        if len(valor) >= 6:
            padroes.append((tipo, valor, re.compile(r"(?<![0-9A-Za-z])" + re.escape(valor) + r"(?![0-9A-Za-z])", re.I)))
        continue
    if len(n) < 6:
        continue
    if tipo == "texto":
        janelas = {n}
    else:
        if tipo == "chave":
            # Os 8 primeiros caracteres (algoritmos) são iguais em toda chave minisign.
            n = norm(valor[8:])
        tam = 12 if tipo == "chave" else (10 if re.fullmatch(r"[0-9a-f]+", n) else 8)
        janelas = {n} if len(n) <= tam else {n[i:i + tam] for i in range(len(n) - tam + 1)}
    # Trechos de baixa entropia (ex.: 00000000, 20202020) aparecem em qualquer lugar; um serial só
    # de zeros não identifica ninguém.
    janelas = {j for j in janelas if len(set(j)) >= 4}
    if janelas:
        padroes.append((tipo, valor, janelas))

# Chave secreta minisign em qualquer lugar (formato da linha base64 de uma chave secreta).
chave_secreta = re.compile(r"RWRTY[A-Za-z0-9+/]{150,}={0,2}")


def documentos():
    """(onde, texto) para cada objeto de cada commit, cada mensagem e cada autor."""
    objs = subprocess.run(["git", "-C", repo, "rev-list", "--objects", "--all"], capture_output=True, text=True, check=True).stdout.split("\n")
    caminho = {}
    for l in objs:
        if " " in l:
            h, p = l.split(" ", 1)
            caminho[h] = p
    if caminho:
        lote = subprocess.run(["git", "-C", repo, "cat-file", "--batch"], input="\n".join(caminho).encode() + b"\n", capture_output=True, check=True).stdout
        pos = 0
        while pos < len(lote):
            fim = lote.index(b"\n", pos)
            h, _, tam = lote[pos:fim].decode().split(" ")
            dados = lote[fim + 1:fim + 1 + int(tam)]
            pos = fim + 1 + int(tam) + 1
            yield caminho[h], dados.decode("utf-8", errors="replace")
    meta = subprocess.run(["git", "-C", repo, "log", "--all", "--format=%h%x00%an <%ae>%x00%cn <%ce>%x00%B%x01"], capture_output=True, text=True, check=True).stdout
    for bloco in meta.split("\x01"):
        if bloco.strip():
            h = bloco.strip().split("\x00")[0]
            yield f"commit {h} (autor/mensagem)", bloco
    # Arquivos ainda sem commit (não ignorados).
    soltos = subprocess.run(["git", "-C", repo, "ls-files", "-o", "--exclude-standard", "-z"], capture_output=True, text=True, check=True).stdout
    for p in filter(None, soltos.split("\0")):
        try:
            yield p + " (sem commit)", open(f"{repo}/{p}", encoding="utf-8", errors="replace").read()
        except OSError:
            pass


achados = {}
for onde, texto in documentos():
    n = norm(texto)
    if chave_secreta.search(texto):
        achados.setdefault(("chave", "RWRT"), set()).add(onde)
    for tipo, valor, busca in padroes:
        if (busca.search(texto) if hasattr(busca, "search") else any(j in n for j in busca)):
            achados.setdefault((tipo, valor[:4]), set()).add(onde)

if achados:
    print("DADOS PRIVADOS NO REPOSITÓRIO (só o começo de cada um):")
    for (tipo, ini), lugares in sorted(achados.items()):
        print(f"  [{tipo}] {ini}… em: {', '.join(sorted(lugares)[:4])}")
    sys.exit(1)
print(f"ok: nenhum dos {len(padroes)} dados privados aparece no histórico")
