//! Eventos de domínio publicados para extensões, e migrações de banco.
use crate::componente::ComponenteId;
use crate::config::Config;
use crate::unidades::WattHora;
use std::collections::BTreeMap;

/// Energia de um minuto fechado (minuto = segundos Unix múltiplos de 60).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RegistroMinuto {
    pub minuto: i64,
    pub energia: BTreeMap<ComponenteId, WattHora>,
    pub segundos_ligado: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventoDeDominio {
    MinutoFechado(RegistroMinuto),
    ConfigAlterada(Config),
}

/// Passo de migração de banco; extensões declaram as suas com tabelas prefixadas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migracao {
    pub versao: u32,
    pub sql: &'static str,
}
