//! Downloads por HTTPS com limite de tamanho, e conferência de integridade.
use crate::ErroAtualizacao;
use crate::manifesto::Pacote;
use crate::versao::Versao;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::time::Duration;

const REPOSITORIO: &str = "https://github.com/gabrielpcruz/kwanto/releases";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origem {
    Oficial,
    /// Base local (testes). Seguro: nada é instalado sem a assinatura da chave embutida.
    Local(String),
}

impl Origem {
    /// `KWANTO_ATUALIZACAO_URL` troca a origem oficial por outra base.
    #[must_use]
    pub fn do_ambiente() -> Self {
        std::env::var("KWANTO_ATUALIZACAO_URL").map_or(Self::Oficial, Self::Local)
    }

    #[must_use]
    pub fn url_manifesto(&self) -> String {
        match self {
            Self::Oficial => format!("{REPOSITORIO}/latest/download/manifesto.json"),
            Self::Local(b) => format!("{}/manifesto.json", b.trim_end_matches('/')),
        }
    }

    #[must_use]
    pub fn url_assinatura(&self) -> String {
        format!("{}.minisig", self.url_manifesto())
    }

    #[must_use]
    pub fn url_pacote(&self, versao: &Versao, arquivo: &str) -> String {
        match self {
            Self::Oficial => format!("{REPOSITORIO}/download/v{versao}/{arquivo}"),
            Self::Local(b) => format!("{}/{arquivo}", b.trim_end_matches('/')),
        }
    }
}

/// Baixa no máximo `limite` bytes; um corpo maior é erro.
pub fn baixar(url: &str, limite: u64) -> Result<Vec<u8>, ErroAtualizacao> {
    let erro = |e: &dyn std::fmt::Display| ErroAtualizacao::Download(e.to_string());
    let agente: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();
    let mut resposta = agente.get(url).call().map_err(|e| erro(&e))?;
    let mut corpo = Vec::new();
    resposta
        .body_mut()
        .with_config()
        .limit(limite.saturating_add(1))
        .reader()
        .take(limite.saturating_add(1))
        .read_to_end(&mut corpo)
        .map_err(|e| erro(&e))?;
    if corpo.len() as u64 > limite {
        return Err(ErroAtualizacao::Download(format!(
            "arquivo maior que {limite} bytes"
        )));
    }
    Ok(corpo)
}

#[must_use]
pub fn sha256_hex(dados: &[u8]) -> String {
    hex::encode(Sha256::digest(dados))
}

pub fn conferir_pacote(dados: &[u8], p: &Pacote) -> Result<(), ErroAtualizacao> {
    if dados.len() as u64 != p.tamanho {
        return Err(ErroAtualizacao::Integridade(format!(
            "tamanho de {} não confere",
            p.arquivo
        )));
    }
    if !sha256_hex(dados).eq_ignore_ascii_case(&p.sha256) {
        return Err(ErroAtualizacao::Integridade(format!(
            "SHA-256 de {} não confere",
            p.arquivo
        )));
    }
    Ok(())
}
