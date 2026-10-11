//! Grava os minutos fechados; se o banco falhar, guarda e tenta de novo no minuto seguinte.
use crate::portas::RepositorioEnergia;
use kwanto_core::eventos::{EventoDeDominio, RegistroMinuto};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::broadcast;

/// Limite da fila: cerca de 1 dia de minutos.
const MAX_PENDENTES: usize = 1440;

pub struct RegistrarMinuto {
    repo: Arc<dyn RepositorioEnergia>,
    eventos: broadcast::Sender<EventoDeDominio>,
    pendentes: VecDeque<RegistroMinuto>,
}

impl std::fmt::Debug for RegistrarMinuto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegistrarMinuto")
            .field("pendentes", &self.pendentes.len())
            .finish_non_exhaustive()
    }
}

impl RegistrarMinuto {
    #[must_use]
    pub fn novo(
        repo: Arc<dyn RepositorioEnergia>,
        eventos: broadcast::Sender<EventoDeDominio>,
    ) -> Self {
        Self {
            repo,
            eventos,
            pendentes: VecDeque::new(),
        }
    }

    #[must_use]
    pub fn pendentes(&self) -> usize {
        self.pendentes.len()
    }

    pub fn executar(&mut self, registro: RegistroMinuto) {
        if self.pendentes.len() == MAX_PENDENTES {
            self.pendentes.pop_front();
            tracing::warn!("fila de minutos pendentes cheia; descartando o mais antigo");
        }
        self.pendentes.push_back(registro);
        while let Some(r) = self.pendentes.front() {
            match self.repo.gravar_minuto(r) {
                Ok(()) => {
                    if let Some(r) = self.pendentes.pop_front() {
                        // sem ouvintes não é erro
                        let _ = self.eventos.send(EventoDeDominio::MinutoFechado(r));
                    }
                }
                Err(e) => {
                    tracing::error!(erro = %e, pendentes = self.pendentes.len(), "não foi possível gravar; tento no próximo minuto");
                    break;
                }
            }
        }
    }
}
