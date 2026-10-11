//! Contratos de extensão: pontos onde outro binário pode acrescentar painéis e dados. Depende só do domínio.
use async_trait::async_trait;
pub use kwanto_core::eventos::{EventoDeDominio, Migracao};
use std::sync::Arc;
use tokio::sync::broadcast;

#[async_trait]
pub trait ExtensaoColeta: Send + Sync {
    fn nome(&self) -> &str;
    /// Migrações próprias, com tabelas prefixadas pelo nome da extensão.
    fn migracoes(&self) -> &'static [Migracao] {
        &[]
    }
    /// Recebe os eventos de domínio. Roda fora da thread de coleta; nunca atrasa a amostragem.
    async fn ao_evento(&self, evento: &EventoDeDominio);
}

/// Entrega cada evento a todas as extensões, em ordem, numa tarefa própria.
pub fn despachar(
    extensoes: Vec<Arc<dyn ExtensaoColeta>>,
    mut eventos: broadcast::Receiver<EventoDeDominio>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match eventos.recv().await {
                Ok(evento) => {
                    for e in &extensoes {
                        e.ao_evento(&evento).await;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(perdidos = n, "extensões atrasadas; eventos descartados");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}
