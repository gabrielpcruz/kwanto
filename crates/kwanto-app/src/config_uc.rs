//! Carregar e atualizar a configuração; a mudança vale na hora via canal `watch`.
use crate::portas::{ErroRepositorio, RepositorioConfig};
use kwanto_core::config::{Config, ESQUEMA};
use kwanto_core::eventos::EventoDeDominio;
use serde_json::{Map, Value};
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

#[derive(Debug, thiserror::Error)]
pub enum ErroAtualizar {
    #[error("{0}")]
    Invalido(String),
    #[error(transparent)]
    Armazenamento(#[from] ErroRepositorio),
}

/// Lê a config gravada; num banco novo grava os padrões (de notebook, se for o caso).
pub fn carregar_config(
    repo: &dyn RepositorioConfig,
    notebook: bool,
) -> Result<Config, ErroRepositorio> {
    let pares = repo.carregar()?;
    let vazio = pares.is_empty();
    let config = Config::de_pares(pares, notebook);
    if vazio {
        repo.salvar(&config.pares())?;
    }
    Ok(config)
}

/// Formato da API: `dia_leitura` como inteiro, os demais como número.
#[must_use]
pub fn config_como_json(c: &Config) -> Map<String, Value> {
    c.pares()
        .into_iter()
        .map(|(k, v)| {
            let inteiro = ESQUEMA.iter().any(|i| i.chave == k && i.inteiro);
            #[allow(clippy::cast_possible_truncation)]
            let valor = if inteiro {
                Value::from(v as i64)
            } else {
                Value::from(v)
            };
            (k.to_string(), valor)
        })
        .collect()
}

fn numero(chave: &str, v: &Value) -> Result<f64, ErroAtualizar> {
    let invalido = || ErroAtualizar::Invalido(format!("{chave}: o valor precisa ser um número"));
    match v {
        Value::Number(n) => n.as_f64().ok_or_else(invalido),
        Value::String(s) => s.trim().replace(',', ".").parse().map_err(|_| invalido()),
        _ => Err(invalido()),
    }
}

pub struct AtualizarConfig {
    repo: Arc<dyn RepositorioConfig>,
    config: watch::Sender<Config>,
    eventos: broadcast::Sender<EventoDeDominio>,
}

impl std::fmt::Debug for AtualizarConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AtualizarConfig")
    }
}

impl AtualizarConfig {
    #[must_use]
    pub fn novo(
        repo: Arc<dyn RepositorioConfig>,
        config: watch::Sender<Config>,
        eventos: broadcast::Sender<EventoDeDominio>,
    ) -> Self {
        Self {
            repo,
            config,
            eventos,
        }
    }

    pub fn executar(&self, corpo: &Map<String, Value>) -> Result<Config, ErroAtualizar> {
        let mut alteracoes = Vec::new();
        for (chave, valor) in corpo {
            if ESQUEMA.iter().any(|i| i.chave == chave) {
                alteracoes.push((chave.clone(), numero(chave, valor)?));
            }
        }
        let atual = self.config.borrow().clone();
        let nova = atual
            .com_alteracoes(&alteracoes)
            .map_err(|e| ErroAtualizar::Invalido(e.to_string()))?;
        self.repo.salvar(&nova.pares())?;
        self.config.send_replace(nova.clone());
        let _ = self
            .eventos
            .send(EventoDeDominio::ConfigAlterada(nova.clone()));
        Ok(nova)
    }
}
