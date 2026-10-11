# Segurança

## Como relatar uma vulnerabilidade

Não abra issue pública. Use o relato privado do GitHub: aba **Security** → **Report a vulnerability**
neste repositório. Respondo em até 7 dias e combino com você a data de divulgação depois da correção.

## Versões cobertas

Só a versão mais recente publicada em Releases recebe correções de segurança. O atualizador da bandeja
oferece a nova versão a quem usa os pacotes .deb e .rpm.

## Como as versões são assinadas

Cada release traz um `manifesto.json` com o SHA-256 dos pacotes, assinado com minisign (ed25519). A
chave pública está em `crates/kwanto-atualizacao/chave-publica.txt`:

    RWSiIoo6IlZdxxw9EaZs2oDCAR+Ao67LG+SRnEeVe7fo06a9NRW09gel

Para conferir à mão: `minisign -Vm manifesto.json -P RWSiIoo6IlZdxxw9EaZs2oDCAR+Ao67LG+SRnEeVe7fo06a9NRW09gel`
e depois `sha256sum` do pacote contra o manifesto. O modelo de ameaça completo está em
[docs/seguranca.md](docs/seguranca.md).
