//! Dois relógios: um que conta o tempo suspenso (intervalos) e o de parede (minutos e datas).
use std::time::Duration;

pub trait Relogio: Send + Sync {
    /// Tempo desde o boot, incluindo suspensão. Só serve para medir intervalos.
    fn intervalo(&self) -> Duration;
    /// Data e hora local.
    fn agora(&self) -> jiff::Zoned;
}

#[cfg(any(test, feature = "testes"))]
pub use falso::RelogioFalso;

#[cfg(any(test, feature = "testes"))]
mod falso {
    use super::Relogio;
    use std::sync::{Mutex, MutexGuard, PoisonError};
    use std::time::Duration;

    /// Relógio controlado pelos testes.
    #[derive(Debug)]
    pub struct RelogioFalso(Mutex<(Duration, jiff::Zoned)>);

    impl RelogioFalso {
        #[must_use]
        pub fn novo(agora: jiff::Zoned) -> Self {
            Self(Mutex::new((Duration::from_secs(1000), agora)))
        }

        fn trava(&self) -> MutexGuard<'_, (Duration, jiff::Zoned)> {
            self.0.lock().unwrap_or_else(PoisonError::into_inner)
        }

        /// Avança os dois relógios.
        pub fn avancar(&self, d: Duration) {
            let mut g = self.trava();
            g.0 += d;
            if let Ok(z) = g.1.checked_add(d) {
                g.1 = z;
            }
        }

        /// Avança só o relógio de intervalo (simula suspensão sem mexer na parede).
        pub fn avancar_so_intervalo(&self, d: Duration) {
            self.trava().0 += d;
        }

        /// Ajusta a hora de parede (simula NTP ou ajuste manual).
        pub fn definir_parede(&self, z: jiff::Zoned) {
            self.trava().1 = z;
        }
    }

    impl Relogio for RelogioFalso {
        fn intervalo(&self) -> Duration {
            self.trava().0
        }
        fn agora(&self) -> jiff::Zoned {
            self.trava().1.clone()
        }
    }
}
