//! Mede o /api/status no pior caso (31 dias cheios, 10 componentes):
//! `cargo run --release -p kwanto-store --example medir_status`.
use kwanto_app::coleta::EstadoAoVivo;
use kwanto_app::portas::RepositorioEnergia;
use kwanto_app::status::{ConsultarStatus, InfoHardware};
use kwanto_core::componente::ComponenteId;
use kwanto_core::config::Config;
use kwanto_core::eventos::RegistroMinuto;
use kwanto_core::relogio::Relogio;
use kwanto_core::unidades::WattHora;
use kwanto_store::ArmazemSqlite;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;

struct Agora(jiff::Zoned);
impl Relogio for Agora {
    fn intervalo(&self) -> Duration {
        Duration::ZERO
    }
    fn agora(&self) -> jiff::Zoned {
        self.0.clone()
    }
}

fn main() {
    let pasta = std::env::temp_dir().join(format!("kwanto-medida-{}", std::process::id()));
    let armazem = Arc::new(ArmazemSqlite::abrir(&pasta.join("k.db"), &[]).expect("banco"));
    let agora = "2026-10-31T23:00:00"
        .parse::<jiff::civil::DateTime>()
        .unwrap()
        .to_zoned(jiff::tz::TimeZone::fixed(jiff::tz::offset(-3)))
        .unwrap();
    let fim = agora.timestamp().as_second() / 60 * 60;
    let inicio = std::time::Instant::now();
    for m in 0..(31 * 1440) {
        let r = RegistroMinuto {
            minuto: fim - m * 60,
            energia: (0..10)
                .map(|c| (ComponenteId::new(format!("c{c}")), WattHora(1.0)))
                .collect(),
            segundos_ligado: 60.0,
        };
        armazem.gravar_minuto(&r).expect("gravar");
    }
    println!(
        "banco com {} minutos montado em {:?}",
        31 * 1440,
        inicio.elapsed()
    );
    let (_, cfg) = watch::channel(Config::padrao(false));
    let (_, est) = watch::channel(Arc::new(EstadoAoVivo::default()));
    let uc = ConsultarStatus::novo(
        armazem.clone(),
        armazem,
        Arc::new(Agora(agora)),
        cfg,
        est,
        InfoHardware {
            cpu_estimada: false,
            tem_hdd: false,
        },
    );
    let mut tempos: Vec<Duration> = (0..20)
        .map(|_| {
            let t = Instant::now();
            uc.executar().expect("status");
            t.elapsed()
        })
        .collect();
    tempos.sort();
    println!("status: mediana {:?}, pior {:?}", tempos[10], tempos[19]);
    let _ = std::fs::remove_dir_all(pasta);
}
