# Publicar uma versão (mantenedor)

As versões são assinadas com uma chave minisign que só existe na máquina do mantenedor. Os pacotes
publicados são compilados nessa máquina, a partir da tag; o CI só testa e abre um rascunho.

## Uma vez

    cargo run -p kwanto-release -- criar-chave

Pede uma senha e grava a chave em `~/.config/kwanto-release/kwanto.key`. Faça backup desse arquivo e
da senha (pendrive, gerenciador de senhas): sem eles, as instalações existentes não aceitam versões
novas. A chave pública vai para `crates/kwanto-atualizacao/chave-publica.txt`; faça commit dela.

## A cada versão

1. Suba a versão em `Cargo.toml` (`[workspace.package] version`) e faça commit.
2. Valide num ambiente limpo e envie com a tag:

       sh scripts/verificar-limpo.sh
       git tag v0.2.0 && git push origin main v0.2.0

3. O CI roda os testes e cria um release em rascunho, sem arquivos.
4. Publique:

       sh scripts/publicar-versao.sh v0.2.0

   Exige docker, nfpm e `gh`. O script compila os pacotes a partir da tag, gera e assina o manifesto
   (pede a senha da chave), confere a assinatura com a chave pública do repositório, confere que os
   binários confiam só na chave oficial, e só então envia os arquivos ao rascunho e o publica. Só a
   partir daí as bandejas oferecem a versão nova.

5. AUR: atualize `pkgver` em `packaging/aur/PKGBUILD`, rode `updpkgsums` e publique no repositório
   do AUR.
