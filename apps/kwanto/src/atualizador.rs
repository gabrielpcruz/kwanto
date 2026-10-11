//! Ciclo do atualizador na bandeja: verifica (2 min após iniciar, depois a cada 24 h), oferece,
//! baixa e confere, e chama o ajudante root pelo pkexec. Nada é instalado sem o usuário aceitar.
use crate::notificacao::{Notificador, Resposta};
use crate::preferencias::Preferencias;
use crate::{Avisar, Evento};
use kwanto_atualizacao::ajudante::CAMINHO_AJUDANTE;
use kwanto_atualizacao::decisao::{Decisao, decidir};
use kwanto_atualizacao::download::{Origem, baixar, conferir_pacote};
use kwanto_atualizacao::formato::{ExecutorReal, FormatoPacote, detectar_formato};
use kwanto_atualizacao::manifesto::{Manifesto, verificar_assinatura};
use kwanto_atualizacao::versao::Versao;
use kwanto_atualizacao::{ErroAtualizacao, LIMITE_BYTES, chave_oficial};
use std::path::{Path, PathBuf};
use std::time::Duration;

const PRIMEIRA_VERIFICACAO: Duration = Duration::from_secs(120);
const INTERVALO: Duration = Duration::from_secs(24 * 3600);

/// Manifesto já verificado de uma versão mais nova, com os bytes exatos que foram assinados.
#[derive(Debug, Clone)]
pub struct Oferta {
    pub manifesto: Manifesto,
    pub bytes: Vec<u8>,
    pub assinatura: String,
    pub versao: Versao,
}

/// Oferta assinada de uma versão mais nova, ou `None` se já está atualizado.
pub fn verificar(origem: &Origem) -> Result<Option<Oferta>, ErroAtualizacao> {
    let bytes = baixar(&origem.url_manifesto(), 1024 * 1024)?;
    let assinatura = String::from_utf8(baixar(&origem.url_assinatura(), 64 * 1024)?)
        .map_err(|_| ErroAtualizacao::Assinatura)?;
    let manifesto = verificar_assinatura(&bytes, &assinatura, chave_oficial())?;
    let versao = manifesto.versao()?;
    Ok((versao > Versao::atual()).then_some(Oferta {
        manifesto,
        bytes,
        assinatura,
        versao,
    }))
}

/// Baixa e confere o pacote; grava manifesto, assinatura e pacote em `dir` para o ajudante.
pub fn preparar(
    origem: &Origem,
    oferta: &Oferta,
    formato: FormatoPacote,
    dir: &Path,
) -> Result<[PathBuf; 3], ErroAtualizacao> {
    let Decisao::Atualizar { versao, pacote } =
        decidir(&oferta.manifesto, Versao::atual(), formato)?
    else {
        return Err(ErroAtualizacao::Recusado(
            "não há pacote mais novo para este sistema".into(),
        ));
    };
    let dados = baixar(&origem.url_pacote(&versao, &pacote.arquivo), LIMITE_BYTES)?;
    conferir_pacote(&dados, &pacote)?;
    let erro = |e: std::io::Error| ErroAtualizacao::Download(e.to_string());
    std::fs::create_dir_all(dir).map_err(erro)?;
    let caminhos = [
        dir.join("manifesto.json"),
        dir.join("manifesto.json.minisig"),
        dir.join(&pacote.arquivo),
    ];
    std::fs::write(&caminhos[0], &oferta.bytes).map_err(erro)?;
    std::fs::write(&caminhos[1], &oferta.assinatura).map_err(erro)?;
    std::fs::write(&caminhos[2], dados).map_err(erro)?;
    Ok(caminhos)
}

/// Espera antes da primeira verificação; `KWANTO_ATUALIZACAO_ESPERA` (segundos) encurta nos
/// testes. Só afeta o momento da verificação, nunca o que é aceito.
fn primeira_espera() -> Duration {
    std::env::var("KWANTO_ATUALIZACAO_ESPERA")
        .ok()
        .and_then(|s| s.parse().ok())
        .map_or(PRIMEIRA_VERIFICACAO, Duration::from_secs)
}

fn pasta_cache() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
        });
    base.join("kwanto/atualizacao")
}

fn erro_dbus(e: zbus::Error) -> ErroAtualizacao {
    ErroAtualizacao::Download(e.to_string())
}

pub async fn ciclo(conn: zbus::Connection, avisar: Avisar) {
    tokio::time::sleep(primeira_espera()).await;
    loop {
        if let Err(e) = rodada(&conn, &avisar).await {
            match e {
                ErroAtualizacao::Assinatura => tracing::warn!(
                    "AVISO DE SEGURANÇA: atualização com assinatura inválida foi ignorada"
                ),
                outro => {
                    tracing::info!(erro = %outro, "não foi possível verificar atualizações agora");
                }
            }
        }
        tokio::time::sleep(INTERVALO).await;
    }
}

async fn rodada(conn: &zbus::Connection, avisar: &Avisar) -> Result<(), ErroAtualizacao> {
    let caminho_prefs = Preferencias::caminho_padrao();
    let mut prefs = Preferencias::carregar(&caminho_prefs);
    if !prefs.verificar {
        return Ok(());
    }
    let origem = Origem::do_ambiente();
    let o = origem.clone();
    let Some(oferta) = tokio::task::spawn_blocking(move || verificar(&o))
        .await
        .map_err(|e| ErroAtualizacao::Download(e.to_string()))??
    else {
        return Ok(());
    };
    let versao = oferta.versao;
    if !prefs.deve_oferecer(&versao) {
        return Ok(());
    }
    let notificador = Notificador::novo(conn).await.map_err(erro_dbus)?;
    let formato =
        tokio::task::spawn_blocking(|| detectar_formato(&ExecutorReal, "/usr/bin/kwanto"))
            .await
            .ok()
            .flatten();
    let Some(formato) = formato else {
        // Instalação manual ou Arch: só avisa.
        let _ = notificador
            .avisar(
                &format!("Kwanto {versao} disponível"),
                "Atualize pelo seu gerenciador de pacotes (no Arch, pelo AUR).",
            )
            .await;
        return Ok(());
    };
    let id = notificador.oferecer(&versao).await.map_err(erro_dbus)?;
    match notificador.esperar_resposta(id).await {
        Resposta::Atualizar => {}
        Resposta::AgoraNao => {
            prefs.pular = Some(versao.to_string());
            let _ = prefs.salvar(&caminho_prefs);
            return Ok(());
        }
        Resposta::NaoPerguntarMais => {
            prefs.verificar = false;
            let _ = prefs.salvar(&caminho_prefs);
            return Ok(());
        }
        Resposta::Fechou => return Ok(()),
    }
    // O ajudante re-verifica tudo como root; aqui só evita chamá-lo com um pacote que não confere.
    let preparado =
        tokio::task::spawn_blocking(move || preparar(&origem, &oferta, formato, &pasta_cache()))
            .await
            .map_err(|e| ErroAtualizacao::Download(e.to_string()))?;
    let caminhos = match preparado {
        Ok(c) => c,
        Err(e) => {
            let _ = std::fs::remove_dir_all(pasta_cache());
            let _ = notificador
                .avisar(
                    "Não foi possível baixar a atualização do Kwanto",
                    "Nada foi alterado; tente de novo mais tarde.",
                )
                .await;
            return Err(e);
        }
    };
    let status = tokio::process::Command::new("/usr/bin/pkexec")
        .arg(CAMINHO_AJUDANTE)
        .args(&caminhos)
        .status()
        .await;
    let _ = std::fs::remove_dir_all(pasta_cache());
    match status {
        Ok(s) if s.success() => {
            let _ = notificador
                .avisar(
                    &format!("Kwanto atualizado para {versao}"),
                    "Pronto. O Kwanto já está na versão nova.",
                )
                .await;
            avisar(Evento::ReiniciarBandeja);
        }
        // pkexec: 126 = autenticação cancelada ou negada.
        Ok(s) if s.code() == Some(126) => {
            tracing::info!("o usuário cancelou a senha da atualização");
        }
        _ => {
            let _ = notificador
                .avisar(
                    "Não foi possível atualizar o Kwanto",
                    "A atualização foi recusada ou falhou. Nada foi alterado; tente de novo mais tarde.",
                )
                .await;
        }
    }
    Ok(())
}
