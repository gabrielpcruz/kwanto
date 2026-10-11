//! Histórico para os gráficos: baldes de 10 min (24 h), 1 h (7 dias) e dia local (30 dias).
use crate::portas::{ErroRepositorio, Intervalo, RepositorioEnergia};
use kwanto_core::config::Config;
use kwanto_core::conta::fator_perda;
use kwanto_core::relogio::Relogio;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Faixa {
    H24,
    D7,
    D30,
}

impl Faixa {
    /// Faixa desconhecida cai em 30 dias, como na versão Python.
    #[must_use]
    pub fn de_texto(t: &str) -> Self {
        match t {
            "24h" => Self::H24,
            "7d" => Self::D7,
            _ => Self::D30,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Ponto {
    pub t: i64,
    #[serde(serialize_with = "crate::arredondar::uma")]
    pub w: f64,
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub kwh: f64,
}

#[derive(Debug, Serialize)]
pub struct Dia {
    pub dia: String,
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub kwh: f64,
    #[serde(serialize_with = "crate::arredondar::duas")]
    pub custo: f64,
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub horas_ligado: f64,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum Historico {
    Linha {
        passo: i64,
        inicio: f64,
        fim: f64,
        pontos: Vec<Ponto>,
    },
    Dias {
        dias: Vec<Dia>,
    },
}

pub struct ConsultarHistorico {
    energia: Arc<dyn RepositorioEnergia>,
    relogio: Arc<dyn Relogio>,
    config: watch::Receiver<Config>,
}

impl std::fmt::Debug for ConsultarHistorico {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConsultarHistorico")
    }
}

impl ConsultarHistorico {
    #[must_use]
    pub fn novo(
        energia: Arc<dyn RepositorioEnergia>,
        relogio: Arc<dyn Relogio>,
        config: watch::Receiver<Config>,
    ) -> Self {
        Self {
            energia,
            relogio,
            config,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn executar(&self, faixa: Faixa) -> Result<Historico, ErroRepositorio> {
        let config = self.config.borrow().clone();
        let fator = 1.0 + fator_perda(config.eficiencia());
        let preco = config.tarifa().efetiva().0;
        let agora = self.relogio.agora();
        let ts = agora.timestamp().as_second();
        let (passo, duracao) = match faixa {
            Faixa::H24 => (600, 86_400),
            Faixa::D7 => (3600, 7 * 86_400),
            Faixa::D30 => (3600, 30 * 86_400),
        };
        let baldes = self.energia.baldes(
            Intervalo {
                inicio: ts - duracao,
                fim: ts + 60,
            },
            passo,
        )?;
        if faixa != Faixa::D30 {
            let pontos = baldes
                .iter()
                .map(|b| {
                    let horas = b.segundos_ligado / 3600.0;
                    let w = if horas > 0.0 {
                        b.wh_dc.0 * fator / horas
                    } else {
                        0.0
                    };
                    Ponto {
                        t: b.inicio,
                        w,
                        kwh: b.wh_dc.0 * fator / 1000.0,
                    }
                })
                .collect();
            return Ok(Historico::Linha {
                passo,
                inicio: (ts - duracao) as f64,
                fim: ts as f64,
                pontos,
            });
        }
        // 30 dias: baldes de 1 h agrupados pelo dia local (fusos do Brasil têm offset de hora inteira).
        let tz = agora.time_zone().clone();
        let mut dias: BTreeMap<jiff::civil::Date, (f64, f64)> = BTreeMap::new();
        for b in &baldes {
            let Ok(t) = jiff::Timestamp::from_second(b.inicio) else {
                continue;
            };
            let e = dias.entry(t.to_zoned(tz.clone()).date()).or_default();
            e.0 += b.wh_dc.0 * fator;
            e.1 += b.segundos_ligado;
        }
        let dias = dias
            .into_iter()
            .map(|(d, (wh, s))| Dia {
                dia: d.to_string(),
                kwh: wh / 1000.0,
                custo: wh / 1000.0 * preco,
                horas_ligado: s / 3600.0,
            })
            .collect();
        Ok(Historico::Dias { dias })
    }
}
