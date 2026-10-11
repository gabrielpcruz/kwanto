//! Portas dos sensores: implementadas por plataforma (Linux hoje, Windows depois).
use crate::componente::{Amostra, Componente, ComponenteId};
use std::time::Duration;

pub trait Medidor: Send {
    fn componentes(&self) -> Vec<Componente>;
    /// Lê todos os componentes deste medidor; `dt` é o tempo desde a leitura anterior.
    fn ler(&mut self, dt: Duration) -> Vec<(ComponenteId, Amostra)>;
    /// Descarta o estado acumulado (chamado após suspensão).
    fn reiniciar(&mut self) {}
}

pub struct Hardware {
    pub medidores: Vec<Box<dyn Medidor>>,
    pub cpu_estimada: bool,
    pub tem_hdd: bool,
    pub notebook: bool,
}

impl std::fmt::Debug for Hardware {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hardware")
            .field("medidores", &self.medidores.len())
            .field("cpu_estimada", &self.cpu_estimada)
            .field("tem_hdd", &self.tem_hdd)
            .field("notebook", &self.notebook)
            .finish()
    }
}

pub trait DetectorDeHardware: Send + Sync {
    fn detectar(&self) -> Hardware;
}
