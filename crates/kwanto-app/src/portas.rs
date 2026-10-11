//! Portas de persistência usadas pelos casos de uso; implementadas pelo kwanto-store.
use kwanto_core::componente::{ComponenteId, DescricaoComponente};
use kwanto_core::eventos::RegistroMinuto;
use kwanto_core::unidades::WattHora;
use std::collections::BTreeMap;

/// Intervalo semiaberto `[inicio, fim)` em segundos Unix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Intervalo {
    pub inicio: i64,
    pub fim: i64,
}

/// Energia DC (sem a perda da fonte) e tempo ligado agregados num balde de tempo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Balde {
    pub inicio: i64,
    pub wh_dc: WattHora,
    pub segundos_ligado: f64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ErroRepositorio {
    #[error("falha de armazenamento: {0}")]
    Armazenamento(String),
}

pub trait RepositorioEnergia: Send + Sync {
    /// Soma ao que já existir no mesmo minuto (reinícios no meio do minuto não perdem energia).
    fn gravar_minuto(&self, registro: &RegistroMinuto) -> Result<(), ErroRepositorio>;
    fn totais_por_componente(
        &self,
        intervalo: Intervalo,
    ) -> Result<BTreeMap<ComponenteId, WattHora>, ErroRepositorio>;
    fn segundos_ligado(&self, intervalo: Intervalo) -> Result<f64, ErroRepositorio>;
    fn primeiro_minuto(&self) -> Result<Option<i64>, ErroRepositorio>;
    /// Baldes alinhados a múltiplos de `passo_s` (em segundos Unix), só os que têm dados, em ordem.
    fn baldes(&self, intervalo: Intervalo, passo_s: i64) -> Result<Vec<Balde>, ErroRepositorio>;
}

pub trait RepositorioConfig: Send + Sync {
    fn carregar(&self) -> Result<Vec<(String, f64)>, ErroRepositorio>;
    fn salvar(&self, pares: &[(&str, f64)]) -> Result<(), ErroRepositorio>;
}

pub trait CatalogoComponentes: Send + Sync {
    /// Insere ou atualiza; componentes que sumiram continuam no catálogo para o histórico.
    fn registrar(&self, componentes: &[DescricaoComponente]) -> Result<(), ErroRepositorio>;
    fn listar(&self) -> Result<Vec<DescricaoComponente>, ErroRepositorio>;
}
