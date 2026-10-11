//! Grandezas físicas e monetárias com tipos próprios, para não somar watts com reais.
use serde::Serialize;
use std::iter::Sum;
use std::ops::{Add, AddAssign};
use std::time::Duration;

macro_rules! grandeza {
    ($nome:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize)]
        #[serde(transparent)]
        pub struct $nome(pub f64);

        impl Add for $nome {
            type Output = Self;
            fn add(self, outro: Self) -> Self {
                Self(self.0 + outro.0)
            }
        }
        impl AddAssign for $nome {
            fn add_assign(&mut self, outro: Self) {
                self.0 += outro.0;
            }
        }
        impl Sum for $nome {
            fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
                iter.fold(Self(0.0), Add::add)
            }
        }
        impl From<$nome> for f64 {
            fn from(v: $nome) -> f64 {
                v.0
            }
        }
    };
}

grandeza!(Watts, "Potência em watts.");
grandeza!(WattHora, "Energia em watt-hora.");
grandeza!(Reais, "Valor em reais.");

/// Preço da energia em R$ por kWh.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default, Serialize)]
#[serde(transparent)]
pub struct ReaisPorKwh(pub f64);

impl Watts {
    /// Energia gasta mantendo esta potência pelo intervalo dado.
    #[must_use]
    pub fn durante(self, dt: Duration) -> WattHora {
        WattHora(self.0 * dt.as_secs_f64() / 3600.0)
    }
}

impl WattHora {
    #[must_use]
    pub fn kwh(self) -> f64 {
        self.0 / 1000.0
    }

    #[must_use]
    pub fn custo(self, tarifa: ReaisPorKwh) -> Reais {
        Reais(self.kwh() * tarifa.0)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::time::Duration;

    #[test]
    fn potencia_durante_tempo_vira_energia() {
        let e = Watts(100.0).durante(Duration::from_secs(36));
        assert!((e.0 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn energia_vezes_tarifa_vira_custo() {
        let custo = WattHora(2000.0).custo(ReaisPorKwh(0.85));
        assert!((custo.0 - 1.70).abs() < 1e-9);
        assert!((WattHora(2500.0).kwh() - 2.5).abs() < 1e-12);
    }

    #[test]
    fn somas() {
        let total: WattHora = [WattHora(1.0), WattHora(2.5)].into_iter().sum();
        assert_eq!(total, WattHora(3.5));
        let mut w = Watts(1.0);
        w += Watts(2.0);
        assert_eq!(w + Watts(1.0), Watts(4.0));
    }
}
