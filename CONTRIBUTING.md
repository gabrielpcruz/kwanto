# Contribuir com o Kwanto

## Compilar

Precisa de Rust (a versão está em `rust-toolchain.toml`; o mínimo suportado é 1.85) e de:

    sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libxdo-dev dbus

Depois:

    cargo build --release

## Rodar em desenvolvimento

O serviço roda com o seu usuário, numa porta separada, e a bandeja aponta para ele:

    KWANTO_DB=/tmp/kwanto-dev.db KWANTO_PORTA=8788 cargo run -p kwantod
    KWANTO_URL=http://127.0.0.1:8788 cargo run -p kwanto

- Sem a regra udev do pacote, o contador de energia do processador só é legível pelo root, então em
  desenvolvimento o processador aparece como estimado.
- Se a bandeja instalada estiver aberta, feche-a antes (`pkill -x kwanto`): só pode haver uma.
- `KWANTO_URL` faz a bandeja aceitar um serviço do próprio usuário. Sem ela, a bandeja só confia no
  serviço do usuário de sistema `kwanto`, o do pacote.

## Validar

Um script faz toda a validação, na mesma ordem e do mesmo jeito que o CI (o CI só chama ele):

    sh scripts/verificar.sh

Ele roda formatação, clippy, testes, os testes dos scripts de instalação e das unidades systemd, o
teste do painel no navegador (precisa do Chrome), a análise das dependências e a barreira de
publicação. As ferramentas fixadas que faltarem são instaladas em `~/.local/bin`.

Antes de enviar, rode a mesma validação num ambiente limpo, igual ao do CI (precisa de docker):

    sh scripts/verificar-limpo.sh

Ela usa só o que está no commit, num Ubuntu 24.04 sem nada da sua máquina, e pega o que só funciona
"aqui": uma ferramenta que só você tem instalada, um arquivo esquecido fora do commit. Um container
usa o kernel da sua máquina, então diferenças de kernel só aparecem no CI, que roda em Ubuntu 22.04
e 24.04.

## Antes do primeiro push

    sh scripts/instalar-hooks.sh

O hook de push recusa commits que não passaram pela validação limpa e procura, em todo o histórico,
identificadores reais da sua máquina (números de série de discos, endereços de rede, nome do PC) e
segredos. Para incluir na busca dados seus que a máquina não conhece (nome completo, endereço, e-mail
pessoal, número de série da placa-mãe), liste-os em `~/.config/kwanto-release/privados.txt`, um por
linha. Esse arquivo nunca vai para o repositório.

## Organização

| Pasta | O que tem |
|---|---|
| `crates/kwanto-core` | domínio: unidades, configuração, estimativas, ciclo da conta |
| `crates/kwanto-app` | casos de uso: coleta, status, histórico, configuração |
| `crates/kwanto-sensors` | leitura do hardware (RAPL, NVML, hwmon, discos) |
| `crates/kwanto-store` | banco SQLite |
| `crates/kwanto-api` | API local e painel |
| `crates/kwanto-atualizacao` | verificação e instalação de versões assinadas |
| `apps/kwantod` | o serviço |
| `apps/kwanto` | a bandeja |
| `apps/kwanto-standby` | o serviço que consulta o standby dos HDDs |
| `apps/kwanto-atualizar` | o ajudante que instala atualizações |
| `web` | painel e resumo da bandeja |
| `packaging`, `scripts` | pacotes, unidades systemd e scripts |

Pacotes `.deb` e `.rpm`: `sh scripts/instalar-nfpm.sh` (uma vez), `sh scripts/gerar-pacotes.sh` e
`sh scripts/testar-pacotes.sh` (instala em containers Ubuntu 22.04 e 24.04, Debian 12 e Fedora).
As imagens do README saem de `sh scripts/gerar-imagens.sh`, com dados fictícios.

Antes de mexer em algo sensível (serviço, API, bandeja, atualizador), leia
[docs/seguranca.md](docs/seguranca.md). Publicar uma versão: [docs/publicar.md](docs/publicar.md).
