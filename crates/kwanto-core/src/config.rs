//! Configuração do usuário. As faixas válidas são dados (servidos ao dashboard), não só código.
use crate::conta::Tarifa;
use crate::unidades::ReaisPorKwh;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemEsquema {
    pub chave: &'static str,
    pub padrao: f64,
    pub padrao_notebook: Option<f64>,
    pub min: f64,
    pub max: f64,
    pub rotulo: &'static str,
    pub inteiro: bool,
}

const fn item(
    chave: &'static str,
    padrao: f64,
    notebook: Option<f64>,
    min: f64,
    max: f64,
    rotulo: &'static str,
) -> ItemEsquema {
    ItemEsquema {
        chave,
        padrao,
        padrao_notebook: notebook,
        min,
        max,
        rotulo,
        inteiro: false,
    }
}

/// Mesmas chaves, faixas e rótulos da versão Python.
pub const ESQUEMA: &[ItemEsquema] = &[
    item(
        "preco_kwh",
        0.85,
        None,
        0.0,
        20.0,
        "Tarifa (R$/kWh, com impostos)",
    ),
    item(
        "bandeira_kwh",
        0.0,
        None,
        0.0,
        5.0,
        "Adicional bandeira (R$/kWh)",
    ),
    ItemEsquema {
        inteiro: true,
        ..item(
            "dia_leitura",
            1.0,
            None,
            1.0,
            28.0,
            "Dia de início do ciclo da conta",
        )
    },
    item(
        "eficiencia_fonte",
        0.88,
        Some(0.90),
        0.5,
        1.0,
        "Eficiência da fonte (0–1)",
    ),
    item(
        "placa_mae_w",
        15.0,
        Some(3.0),
        0.0,
        200.0,
        "Placa-mãe/chipset (W)",
    ),
    item("ram_w", 8.0, Some(1.5), 0.0, 100.0, "RAM (W)"),
    item(
        "fans_usb_w",
        8.0,
        Some(1.0),
        0.0,
        200.0,
        "Fans, bomba, USB, LEDs (W)",
    ),
    item("hdd_ocioso_w", 4.0, None, 0.0, 30.0, "HDD ocioso (W)"),
    item("hdd_ativo_w", 6.0, None, 0.0, 30.0, "HDD ativo (W)"),
    item("hdd_standby_w", 0.8, None, 0.0, 30.0, "HDD em standby (W)"),
    item("ssd_ocioso_w", 1.5, Some(0.5), 0.0, 30.0, "SSD ocioso (W)"),
    item("ssd_ativo_w", 5.5, Some(3.5), 0.0, 30.0, "SSD ativo (W)"),
    item(
        "cpu_ocioso_w",
        10.0,
        Some(3.0),
        0.0,
        500.0,
        "CPU ociosa (W)",
    ),
    item(
        "cpu_max_w",
        65.0,
        Some(28.0),
        0.0,
        500.0,
        "CPU em carga total (W)",
    ),
];

fn esquema(chave: &str) -> Option<&'static ItemEsquema> {
    ESQUEMA.iter().find(|i| i.chave == chave)
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ErroConfig {
    #[error("{rotulo}: fora do intervalo [{min}, {max}]")]
    ForaDaFaixa {
        rotulo: &'static str,
        min: f64,
        max: f64,
    },
    #[error("{rotulo}: o valor precisa ser um número")]
    NaoFinito { rotulo: &'static str },
    #[error("{rotulo}: o valor precisa ser um número inteiro")]
    NaoInteiro { rotulo: &'static str },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    valores: BTreeMap<&'static str, f64>,
}

impl Config {
    #[must_use]
    pub fn padrao(notebook: bool) -> Self {
        let valores = ESQUEMA
            .iter()
            .map(|i| {
                let v = if notebook {
                    i.padrao_notebook.unwrap_or(i.padrao)
                } else {
                    i.padrao
                };
                (i.chave, v)
            })
            .collect();
        Self { valores }
    }

    /// Monta a partir do que estava gravado: completa chaves faltantes e ignora desconhecidas.
    pub fn de_pares(pares: impl IntoIterator<Item = (String, f64)>, notebook: bool) -> Self {
        let mut c = Self::padrao(notebook);
        for (chave, valor) in pares {
            if let Some(i) = esquema(&chave) {
                c.valores.insert(i.chave, valor);
            }
        }
        c
    }

    /// Aplica alterações validadas. Chaves desconhecidas são ignoradas (paridade com a versão Python).
    pub fn com_alteracoes(&self, alteracoes: &[(String, f64)]) -> Result<Self, ErroConfig> {
        let mut nova = self.clone();
        for (chave, valor) in alteracoes {
            let Some(i) = esquema(chave) else { continue };
            if !valor.is_finite() {
                return Err(ErroConfig::NaoFinito { rotulo: i.rotulo });
            }
            if i.inteiro && valor.fract() != 0.0 {
                return Err(ErroConfig::NaoInteiro { rotulo: i.rotulo });
            }
            if !(i.min..=i.max).contains(valor) {
                return Err(ErroConfig::ForaDaFaixa {
                    rotulo: i.rotulo,
                    min: i.min,
                    max: i.max,
                });
            }
            nova.valores.insert(i.chave, *valor);
        }
        Ok(nova)
    }

    #[must_use]
    pub fn valor(&self, chave: &str) -> Option<f64> {
        self.valores.get(chave).copied()
    }

    fn v(&self, chave: &str) -> f64 {
        self.valor(chave).unwrap_or_default()
    }

    #[must_use]
    pub fn pares(&self) -> Vec<(&'static str, f64)> {
        self.valores.iter().map(|(k, v)| (*k, *v)).collect()
    }

    #[must_use]
    pub fn tarifa(&self) -> Tarifa {
        Tarifa {
            preco: ReaisPorKwh(self.v("preco_kwh")),
            bandeira: ReaisPorKwh(self.v("bandeira_kwh")),
        }
    }

    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn dia_leitura(&self) -> u8 {
        self.v("dia_leitura").clamp(1.0, 28.0) as u8
    }

    #[must_use]
    pub fn eficiencia(&self) -> f64 {
        self.v("eficiencia_fonte").clamp(0.5, 1.0)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn padroes_de_desktop_e_notebook() {
        let d = Config::padrao(false);
        let n = Config::padrao(true);
        assert_eq!(d.valor("placa_mae_w"), Some(15.0));
        assert_eq!(n.valor("placa_mae_w"), Some(3.0));
        assert_eq!(n.valor("preco_kwh"), Some(0.85)); // sem padrão de notebook: usa o normal
        assert_eq!(d.dia_leitura(), 1);
        assert_eq!(ESQUEMA.len(), 14);
    }

    #[test]
    fn de_pares_completa_faltantes_e_ignora_desconhecidas() {
        let c = Config::de_pares(vec![("preco_kwh".into(), 1.1), ("xyz".into(), 9.0)], false);
        assert_eq!(c.valor("preco_kwh"), Some(1.1));
        assert_eq!(c.valor("ram_w"), Some(8.0));
        assert_eq!(c.valor("xyz"), None);
    }

    #[test]
    fn alteracao_valida_e_invalida() {
        let c = Config::padrao(false);
        let nova = c.com_alteracoes(&[("preco_kwh".into(), 0.95)]).unwrap();
        assert_eq!(nova.valor("preco_kwh"), Some(0.95));

        let erro = c
            .com_alteracoes(&[("eficiencia_fonte".into(), 1.5)])
            .unwrap_err();
        assert_eq!(
            erro.to_string(),
            "Eficiência da fonte (0–1): fora do intervalo [0.5, 1]"
        );
        assert!(matches!(
            c.com_alteracoes(&[("ram_w".into(), f64::NAN)]),
            Err(ErroConfig::NaoFinito { .. })
        ));
        assert!(matches!(
            c.com_alteracoes(&[("dia_leitura".into(), 2.5)]),
            Err(ErroConfig::NaoInteiro { .. })
        ));
    }

    #[test]
    fn alteracao_com_chave_desconhecida_e_ignorada() {
        let c = Config::padrao(false);
        assert_eq!(c.com_alteracoes(&[("nao_existe".into(), 1.0)]).unwrap(), c);
    }

    #[test]
    fn tarifa_efetiva_soma_bandeira() {
        let c = Config::padrao(false)
            .com_alteracoes(&[("bandeira_kwh".into(), 0.05)])
            .unwrap();
        assert!((c.tarifa().efetiva().0 - 0.90).abs() < 1e-9);
    }
}
