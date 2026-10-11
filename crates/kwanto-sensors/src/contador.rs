//! Converte um contador de energia acumulada (µJ) em potência média.
use crate::raiz::ler_u64;
use kwanto_core::unidades::Watts;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug)]
pub struct Contador {
    arquivo: PathBuf,
    maximo: Option<u64>,
    ultimo: Option<u64>,
}

impl Contador {
    #[must_use]
    pub fn novo(arquivo: PathBuf, maximo: Option<u64>) -> Self {
        let ultimo = ler_u64(&arquivo);
        Self {
            arquivo,
            maximo,
            ultimo,
        }
    }

    #[must_use]
    pub fn legivel(&self) -> bool {
        self.ultimo.is_some()
    }

    /// Potência média desde a leitura anterior; trata a virada do contador quando há máximo.
    pub fn ler(&mut self, dt: Duration) -> Option<Watts> {
        let atual = ler_u64(&self.arquivo);
        let anterior = std::mem::replace(&mut self.ultimo, atual);
        let (atual, anterior) = (atual?, anterior?);
        if dt.is_zero() {
            return None;
        }
        let delta = if atual >= anterior {
            atual - anterior
        } else {
            self.maximo? - anterior + atual
        };
        #[allow(clippy::cast_precision_loss)]
        Some(Watts(delta as f64 / 1e6 / dt.as_secs_f64()))
    }

    pub fn reiniciar(&mut self) {
        self.ultimo = ler_u64(&self.arquivo);
    }
}
