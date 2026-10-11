# Segurança do Kwanto

O Kwanto roda em três partes: o serviço `kwantod` (lê os sensores, guarda o histórico e serve o painel em
`http://127.0.0.1:8787`), o serviço `kwanto-standby` (só consulta se os HDDs estão parados) e a bandeja `kwanto`, na
sessão de quem usa o PC. Este documento diz o que cada parte pode fazer, o que um atacante ganharia e por quê.

## O que cada parte pode fazer

| Parte | Usuário | Privilégios | Rede |
|---|---|---|---|
| `kwantod` | `kwanto` (de sistema) | nenhum: sem capability, sem grupo `disk`, sem acesso a nenhum disco (`DevicePolicy=closed`, só os dispositivos da NVIDIA) | escuta só em 127.0.0.1 |
| `kwanto-standby` | `kwanto-standby` (de sistema, separado) | grupo `disk` e `CAP_SYS_RAWIO`, só leitura em `/dev/sd*` | nenhuma (`PrivateNetwork=yes`) |
| bandeja `kwanto` | você | os seus | só 127.0.0.1 e o GitHub (atualizações) |
| `kwanto-atualizar` | root, via polkit | instala o pacote | nenhuma |

- **Contador de energia da CPU (RAPL):** o kernel deixa `energy_uj` legível só pelo root (CVE-2020-8694, "Platypus":
  leituras finas e frequentes vazam informação de outros processos). A regra udev `60-kwanto.rules` libera a leitura só
  ao grupo `kwanto`. O serviço lê a cada 2 s e a API mostra médias arredondadas a 0,1 W; extensões não podem expor
  energia da CPU com resolução menor que 1 s.
- **Standby do HDD:** consultar o disco exige o grupo `disk` e `CAP_SYS_RAWIO`. Com eles, um processo comprometido
  poderia ler e **escrever** em qualquer disco SATA/USB, mesmo com o dispositivo aberto só para leitura (o ioctl
  `SG_IO` aceita comandos de escrita de quem tem essa capability). Por isso essa consulta fica num processo mínimo,
  sem rede, que não recebe nada de fora: ele só grava `/run/kwanto-standby/estado.json` (`sda: standby/ativo`,
  legível só pelo grupo `kwanto`) e o `kwantod` só lê esse arquivo. Um `kwantod` comprometido não alcança os discos, nem o processo que os alcança (usuário
  diferente).
- **Banco de dados:** `/var/lib/kwanto/kwanto.db`, do usuário `kwanto`, permissão 0600 (`UMask=0077`).

## A API local e o painel

- **Só o usuário diante da máquina usa a API.** Para cada conexão, o serviço descobre no kernel (`/proc/net/tcp`) qual
  usuário abriu o socket do outro lado e só responde em `/api/*` se for o root, o próprio usuário do serviço (`kwanto`,
  que já é dono do banco) ou um usuário com sessão **ativa, local e num seat** no `logind`. Só valem conexões vivas
  (estado ESTABLISHED): um socket fechado pelo cliente fica órfão e o kernel o atribui ao root, então ele é tratado
  como "dono desconhecido". Outro usuário da máquina, uma sessão SSH ou um processo de outro usuário recebem
  403. Sem `logind` ou sem conseguir identificar a conexão, a resposta é 403 (falha fechada). O histórico por minuto
  revela quando o PC é usado, por isso a leitura também é protegida. As páginas estáticas (`/`, `/mini`) continuam
  abertas: sem a API, elas não mostram dados.
- **Sites no navegador:** o cabeçalho `Host` precisa ser `127.0.0.1:<porta>` ou `localhost:<porta>` (contra DNS
  rebinding); POST exige `Content-Type: application/json` (força a verificação CORS do navegador) e `Origin`, se
  houver, do próprio serviço; nenhuma resposta tem cabeçalhos CORS. Um site aberto no navegador não lê nem altera nada.
- **Texto vindo do hardware:** nomes de discos e placas vêm do sysfs e do driver, e um dispositivo conectado pode
  mentir o próprio nome. Os rótulos são limpos na entrada e de novo na saída (sem `<>"'&\``, sem caracteres de
  controle, até 64 caracteres), o painel nunca transforma dados em HTML sem escapar, e todas as respostas levam
  `Content-Security-Policy` com `script-src 'self'` (nenhum script inline ou de fora roda), além de `nosniff`,
  `X-Frame-Options: DENY` e `Referrer-Policy: no-referrer`. Painéis de extensão só podem apontar para `/api/...`.
- **Identificadores do hardware:** a API não mostra número de série, WWID nem UUID: discos e placas aparecem com um
  ID derivado (`disco:id-<16 hex de SHA-256>`). Bancos de versões anteriores são convertidos na primeira execução, sem
  perder o histórico. Observação: qualquer usuário local já consegue ler esses identificadores em `/sys`; o objetivo é
  não vazá-los em capturas de tela, relatos de erro ou respostas copiadas.
- **O que a API permite mudar:** só os 14 números de configuração (tarifa e watts estimados), validados por faixa.
  Nenhuma rota executa comandos, lê arquivos ou aceita caminhos.

## A bandeja

- **O popup não carrega nada da rede.** A página do popup está embutida no binário da bandeja e é servida por um
  protocolo próprio (`kwanto://app/`). O popup só navega dentro dessa origem, recusa abrir outras janelas e só aceita
  mensagens (IPC) vindas dela.
- **A bandeja confere quem é o serviço.** Antes de usar qualquer resposta, ela confere no kernel que o socket do outro
  lado da conexão pertence ao usuário `kwanto`, na mesma conexão em que faz o pedido (sem janela entre conferir e usar).
  Se outro programa ocupar a porta 8787 (por exemplo durante o boot, antes do serviço subir), a bandeja trata o serviço
  como parado, registra um aviso de segurança e não mostra nada que venha dele. O botão "Abrir dashboard" só abre o
  navegador quando o serviço foi conferido.
- `KWANTO_URL` (para desenvolvimento) aceita só `http://127.0.0.1:<porta>` ou `http://localhost:<porta>`; com ela, a
  bandeja também aceita um serviço do próprio usuário.

## O atualizador

A bandeja confere atualizações no GitHub Releases, mas só instala pacote cujo manifesto foi assinado (minisign, Ed25519)
com a chave do mantenedor. A chave privada fica só na máquina do mantenedor, cifrada por senha; o CI nunca a vê. Os
pacotes publicados também são compilados na máquina do mantenedor, a partir da tag, com ferramentas fixadas por hash, e
conferidos antes do envio: a assinatura com a chave pública do repositório, e os binários, que precisam confiar só na
chave oficial. Nada produzido pelo CI é assinado. A chave pública está embutida nos binários e só muda numa nova
compilação.

| Ameaça | Defesa |
|---|---|
| Conta do GitHub invadida publica pacote malicioso | Assinatura com chave que nunca está no GitHub; bandeja e ajudante recusam sem ela |
| CI ou action comprometida troca os pacotes do release | Os pacotes assinados são compilados localmente a partir da tag; o CI só testa e abre um rascunho vazio, sem token de escrita nos passos que compilam; actions fixadas por commit |
| Binário publicado com uma chave de teste embutida | `publicar-versao.sh` recusa rodar com `KWANTO_CHAVE_PUBLICA` definida e confere a chave dentro dos binários do pacote |
| Mantenedor assina com a chave errada | `publicar-versao.sh` confere a assinatura com a chave pública do repositório antes de enviar |
| Troca de arquivo entre verificação e instalação | O ajudante (root) copia para um diretório só seu (`/var/cache/kwanto-atualizar/<aleatório>`, 0700) e verifica e instala a cópia |
| Caminho malicioso passado ao ajudante (link simbólico, FIFO, dispositivo, arquivo enorme) | Só aceita arquivo comum, até 200 MB, sem seguir link simbólico no último componente do caminho; nenhum conteúdo aparece em mensagens de erro |
| Rebaixamento para versão antiga vulnerável | A bandeja não oferece e o ajudante recusa versão que não seja maior que a instalada |
| Rede interceptada | HTTPS + assinatura + SHA-256 e tamanho de cada pacote |
| Processo do usuário chama o ajudante sozinho | O polkit exige a senha de administrador a cada vez; mesmo assim o ajudante só instala pacote assinado e mais novo |
| `KWANTO_ATUALIZACAO_URL` aponta para outro servidor | Só troca de onde se baixa; a assinatura continua exigida com a chave embutida |
| Sistema sem pacote deb/rpm (manual, Arch) | A bandeja só avisa que há versão nova; nunca chama o ajudante |
| Gerenciador de pacotes ocupado ou arquivo de configuração alterado | `apt-get` espera a trava por até 5 min, mantém as configurações do usuário e não pergunta nada |
| Manifesto antigo reapresentado (congelamento) | Só atrasa atualizações; aceito |
| Perda da chave | Backup obrigatório; sem ela, novas versões exigem reinstalação manual uma vez |

O ajudante nunca usa shell: chama `/usr/bin/pkexec`, `apt-get`, `dnf` ou `zypper` com argumentos separados.

## Publicação do código

Antes de qualquer `git push`, o hook instalado por `scripts/instalar-hooks.sh` exige que o commit tenha passado por
`scripts/verificar-limpo.sh` (a validação completa num Ubuntu limpo, igual ao CI) e roda `scripts/auditar-publicacao.sh`,
que procura em todo o histórico (inclusive arquivos binários, mensagens e autores) os identificadores reais da máquina
de quem publica (seriais e WWID de discos, MACs, UUIDs, machine-id, nome do PC, redes), dados privados listados em
`~/.config/kwanto-release/privados.txt`, qualquer trecho da chave privada de assinatura, arquivos rastreados em caminhos
ignorados e segredos (`gitleaks`). O CI roda a mesma barreira em todo push.

## Relatar uma vulnerabilidade

Veja [SECURITY.md](../SECURITY.md).
