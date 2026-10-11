//! Manifesto de uma versão: lista os pacotes com SHA-256 e tamanho. Só é aceito com assinatura
//! válida da chave embutida.
use crate::versao::Versao;
use crate::{ErroAtualizacao, LIMITE_BYTES};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Pacote {
    pub arquivo: String,
    pub sha256: String,
    pub tamanho: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifesto {
    pub versao: String,
    pub pacotes: BTreeMap<String, Pacote>,
}

impl Manifesto {
    pub fn versao(&self) -> Result<Versao, ErroAtualizacao> {
        Versao::ler(&self.versao)
    }

    #[must_use]
    pub fn pacote_por_chave(&self, chave: &str) -> Option<&Pacote> {
        self.pacotes.get(chave)
    }

    #[must_use]
    pub fn pacote(&self, formato: crate::formato::FormatoPacote) -> Option<&Pacote> {
        self.pacotes.get(formato.chave())
    }

    /// Nome de arquivo simples, SHA-256 com 64 dígitos hexadecimais e tamanho dentro do limite.
    fn validar(&self) -> Result<(), ErroAtualizacao> {
        self.versao()?;
        for p in self.pacotes.values() {
            let nome_ok = !p.arquivo.is_empty()
                && !p.arquivo.starts_with('.')
                && p.arquivo
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c));
            if !nome_ok {
                return Err(ErroAtualizacao::Manifesto(format!(
                    "nome de arquivo inválido: {}",
                    p.arquivo
                )));
            }
            if p.sha256.len() != 64 || !p.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
                return Err(ErroAtualizacao::Manifesto(format!(
                    "SHA-256 inválido para {}",
                    p.arquivo
                )));
            }
            if p.tamanho == 0 || p.tamanho > LIMITE_BYTES {
                return Err(ErroAtualizacao::Manifesto(format!(
                    "tamanho inválido para {}",
                    p.arquivo
                )));
            }
        }
        Ok(())
    }
}

/// Confere a assinatura antes de ler qualquer campo; só então interpreta e valida o manifesto.
pub fn verificar_assinatura(
    manifesto: &[u8],
    assinatura: &str,
    chave_publica: &str,
) -> Result<Manifesto, ErroAtualizacao> {
    let chave = minisign_verify::PublicKey::decode(chave_publica.trim())
        .map_err(|_| ErroAtualizacao::Assinatura)?;
    let sig = minisign_verify::Signature::decode(assinatura.trim())
        .map_err(|_| ErroAtualizacao::Assinatura)?;
    chave
        .verify(manifesto, &sig, false)
        .map_err(|_| ErroAtualizacao::Assinatura)?;
    let m: Manifesto =
        serde_json::from_slice(manifesto).map_err(|e| ErroAtualizacao::Manifesto(e.to_string()))?;
    m.validar()?;
    Ok(m)
}
