//! Converte o que os medidores informam (atividade, standby, fixo) em watts.
use crate::componente::{Amostra, Perfil};
use crate::config::Config;
use crate::unidades::Watts;

/// Acima disso a leitura é tratada como defeito do sensor.
pub const POTENCIA_MAXIMA: Watts = Watts(5000.0);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParametrosDeEstimativa {
    pub cpu_ocioso: Watts,
    pub cpu_max: Watts,
    pub hdd_ocioso: Watts,
    pub hdd_ativo: Watts,
    pub hdd_standby: Watts,
    pub ssd_ocioso: Watts,
    pub ssd_ativo: Watts,
    pub ram: Watts,
    pub placa_mae: Watts,
    pub fans_usb: Watts,
}

impl ParametrosDeEstimativa {
    #[must_use]
    pub fn de_config(c: &Config) -> Self {
        let w = |k: &str| Watts(c.valor(k).unwrap_or_default());
        Self {
            cpu_ocioso: w("cpu_ocioso_w"),
            cpu_max: w("cpu_max_w"),
            hdd_ocioso: w("hdd_ocioso_w"),
            hdd_ativo: w("hdd_ativo_w"),
            hdd_standby: w("hdd_standby_w"),
            ssd_ocioso: w("ssd_ocioso_w"),
            ssd_ativo: w("ssd_ativo_w"),
            ram: w("ram_w"),
            placa_mae: w("placa_mae_w"),
            fans_usb: w("fans_usb_w"),
        }
    }

    fn faixa(&self, perfil: Perfil) -> (Watts, Watts) {
        match perfil {
            Perfil::Cpu => (self.cpu_ocioso, self.cpu_max),
            Perfil::Hdd => (self.hdd_ocioso, self.hdd_ativo),
            Perfil::Ssd => (self.ssd_ocioso, self.ssd_ativo),
            Perfil::Ram => (self.ram, self.ram),
            Perfil::PlacaMae => (self.placa_mae, self.placa_mae),
            Perfil::FansUsb => (self.fans_usb, self.fans_usb),
            Perfil::Sensor => (Watts(0.0), Watts(0.0)),
        }
    }
}

fn plausivel(w: Watts) -> Option<Watts> {
    (w.0.is_finite() && (0.0..=POTENCIA_MAXIMA.0).contains(&w.0)).then_some(w)
}

/// Potência de um componente a partir da amostra; `None` quando não há leitura válida.
#[must_use]
pub fn estimar(perfil: Perfil, amostra: Amostra, p: &ParametrosDeEstimativa) -> Option<Watts> {
    let (ocioso, ativo) = p.faixa(perfil);
    match amostra {
        Amostra::Potencia(w) => plausivel(w),
        Amostra::SemLeitura => None,
        Amostra::Atividade(f) => {
            let f = if f.is_finite() {
                f.clamp(0.0, 1.0)
            } else {
                0.0
            };
            plausivel(Watts(ocioso.0 + (ativo.0 - ocioso.0) * f))
        }
        Amostra::Standby if perfil == Perfil::Hdd => plausivel(p.hdd_standby),
        Amostra::Standby | Amostra::Fixo => plausivel(ocioso),
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::config::Config;

    fn p() -> ParametrosDeEstimativa {
        ParametrosDeEstimativa::de_config(&Config::padrao(false))
    }

    #[test]
    fn potencia_medida_passa_e_absurda_vira_none() {
        assert_eq!(
            estimar(Perfil::Sensor, Amostra::Potencia(Watts(120.0)), &p()),
            Some(Watts(120.0))
        );
        assert_eq!(
            estimar(Perfil::Sensor, Amostra::Potencia(Watts(-1.0)), &p()),
            None
        );
        assert_eq!(
            estimar(Perfil::Sensor, Amostra::Potencia(Watts(6000.0)), &p()),
            None
        );
        assert_eq!(
            estimar(Perfil::Sensor, Amostra::Potencia(Watts(f64::NAN)), &p()),
            None
        );
        assert_eq!(estimar(Perfil::Sensor, Amostra::SemLeitura, &p()), None);
    }

    #[test]
    fn atividade_interpola_entre_ocioso_e_ativo() {
        // HDD: 4 W ocioso, 6 W ativo
        assert_eq!(
            estimar(Perfil::Hdd, Amostra::Atividade(0.5), &p()),
            Some(Watts(5.0))
        );
        // SSD: 1,5 → 5,5; atividade fora de 0..1 é limitada
        assert_eq!(
            estimar(Perfil::Ssd, Amostra::Atividade(2.0), &p()),
            Some(Watts(5.5))
        );
        // CPU: 10 → 65
        assert_eq!(
            estimar(Perfil::Cpu, Amostra::Atividade(0.0), &p()),
            Some(Watts(10.0))
        );
    }

    #[test]
    fn standby_e_fixos() {
        assert_eq!(
            estimar(Perfil::Hdd, Amostra::Standby, &p()),
            Some(Watts(0.8))
        );
        assert_eq!(estimar(Perfil::Ram, Amostra::Fixo, &p()), Some(Watts(8.0)));
        assert_eq!(
            estimar(Perfil::PlacaMae, Amostra::Fixo, &p()),
            Some(Watts(15.0))
        );
        assert_eq!(
            estimar(Perfil::FansUsb, Amostra::Fixo, &p()),
            Some(Watts(8.0))
        );
    }
}
