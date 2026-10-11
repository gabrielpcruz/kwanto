//! Atualização segura do Kwanto: versões, manifesto assinado (minisign), decisão e as regras do
//! ajudante que instala como root. Sem rede nem processos aqui, exceto em `download`.
pub mod ajudante;
pub mod decisao;
pub mod download;
pub mod formato;
pub mod manifesto;
pub mod versao;

/// Nenhum arquivo baixado ou copiado pode passar disso.
pub const LIMITE_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ErroAtualizacao {
    #[error("a assinatura da atualização não confere")]
    Assinatura,
    #[error("manifesto inválido: {0}")]
    Manifesto(String),
    #[error("versão inválida: {0}")]
    Versao(String),
    #[error("falha no download: {0}")]
    Download(String),
    #[error("o arquivo não confere com o manifesto: {0}")]
    Integridade(String),
    #[error("atualização recusada: {0}")]
    Recusado(String),
}

/// Chave pública que assina as versões oficiais. Só pode ser trocada na compilação
/// (`KWANTO_CHAVE_PUBLICA`), nunca em tempo de execução.
#[must_use]
pub fn chave_oficial() -> &'static str {
    option_env!("KWANTO_CHAVE_PUBLICA").unwrap_or(include_str!("../chave-publica.txt"))
}
