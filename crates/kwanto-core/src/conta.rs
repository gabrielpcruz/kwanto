//! Tarifa, ciclo da conta, perda da fonte e previsão.
use crate::unidades::ReaisPorKwh;
use jiff::ToSpan;
use jiff::civil::Date;

/// Preço da energia: tarifa com impostos + adicional da bandeira tarifária.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tarifa {
    pub preco: ReaisPorKwh,
    pub bandeira: ReaisPorKwh,
}

impl Tarifa {
    #[must_use]
    pub fn efetiva(self) -> ReaisPorKwh {
        ReaisPorKwh(self.preco.0 + self.bandeira.0)
    }
}

/// Período de leitura da conta de luz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CicloDaConta {
    pub inicio: Date,
    pub fim: Date,
}

impl CicloDaConta {
    /// Ciclo que contém `hoje`, começando no `dia_leitura` (1..=28) de cada mês.
    #[must_use]
    pub fn contendo(hoje: Date, dia_leitura: u8) -> Self {
        let dia = i8::try_from(dia_leitura.clamp(1, 28)).unwrap_or(1);
        let neste_mes = Date::new(hoje.year(), hoje.month(), dia).unwrap_or(hoje);
        let inicio = if hoje >= neste_mes {
            neste_mes
        } else {
            neste_mes.checked_sub(1.month()).unwrap_or(neste_mes)
        };
        let fim = inicio.checked_add(1.month()).unwrap_or(inicio);
        Self { inicio, fim }
    }
}

/// Quanto se perde na fonte para cada unidade entregue aos componentes.
#[must_use]
pub fn fator_perda(eficiencia: f64) -> f64 {
    1.0 / eficiencia.clamp(0.5, 1.0) - 1.0
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Previsao {
    pub media_kwh_hora: f64,
    pub previsto_kwh: f64,
}

/// Previsão do ciclo: o consumido até agora + média por hora de relógio × horas restantes.
#[must_use]
pub fn prever(
    kwh_ciclo: f64,
    kwh_janela: f64,
    horas_janela: f64,
    horas_restantes: f64,
) -> Previsao {
    let media_kwh_hora = kwh_janela / horas_janela.max(1.0 / 60.0);
    Previsao {
        media_kwh_hora,
        previsto_kwh: kwh_ciclo + media_kwh_hora * horas_restantes.max(0.0),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use jiff::civil::date;

    #[test]
    fn ciclo_do_mes_corrente_e_do_anterior() {
        let c = CicloDaConta::contendo(date(2026, 10, 10), 5);
        assert_eq!((c.inicio, c.fim), (date(2026, 10, 5), date(2026, 11, 5)));
        let c = CicloDaConta::contendo(date(2026, 10, 3), 5);
        assert_eq!((c.inicio, c.fim), (date(2026, 9, 5), date(2026, 10, 5)));
    }

    #[test]
    fn ciclo_vira_o_ano() {
        let c = CicloDaConta::contendo(date(2027, 1, 2), 10);
        assert_eq!((c.inicio, c.fim), (date(2026, 12, 10), date(2027, 1, 10)));
        let c = CicloDaConta::contendo(date(2026, 12, 20), 10);
        assert_eq!((c.inicio, c.fim), (date(2026, 12, 10), date(2027, 1, 10)));
    }

    #[test]
    fn perda_da_fonte() {
        assert!((fator_perda(0.88) - 0.136_363_636).abs() < 1e-6);
        assert!(fator_perda(1.0).abs() < 1e-12);
    }

    #[test]
    fn previsao_soma_media_vezes_horas_restantes() {
        let p = prever(10.0, 2.4, 24.0, 100.0);
        assert!((p.media_kwh_hora - 0.1).abs() < 1e-12);
        assert!((p.previsto_kwh - 20.0).abs() < 1e-12);
    }

    #[test]
    fn previsao_com_janela_minuscula_usa_piso_de_um_minuto() {
        let p = prever(0.0, 0.01, 0.0, 1.0);
        assert!((p.media_kwh_hora - 0.6).abs() < 1e-9);
    }
}
