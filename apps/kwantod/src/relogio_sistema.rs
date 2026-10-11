//! Relógio real: intervalo pelo CLOCK_BOOTTIME (conta a suspensão) e hora local pelo jiff.
use kwanto_core::relogio::Relogio;
use std::time::Duration;

#[derive(Debug, Default)]
pub struct RelogioSistema;

impl Relogio for RelogioSistema {
    fn intervalo(&self) -> Duration {
        let t = rustix::time::clock_gettime(rustix::time::ClockId::Boottime);
        Duration::new(
            u64::try_from(t.tv_sec).unwrap_or(0),
            u32::try_from(t.tv_nsec).unwrap_or(0),
        )
    }

    fn agora(&self) -> jiff::Zoned {
        jiff::Zoned::now()
    }
}
