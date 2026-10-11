use jiff::Zoned;
use jiff::tz::TimeZone;
use kwanto_app::coleta::{Acumulador, Amostrador, EstadoAoVivo};
use kwanto_core::componente::{Amostra, Componente, ComponenteId, Perfil};
use kwanto_core::config::Config;
use kwanto_core::relogio::RelogioFalso;
use kwanto_core::sensores::Medidor;
use kwanto_core::unidades::{WattHora, Watts};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::watch;

fn hora(s: &str) -> Zoned {
    s.parse::<jiff::civil::DateTime>()
        .unwrap()
        .to_zoned(TimeZone::fixed(jiff::tz::offset(-3)))
        .unwrap()
}

fn w(pares: &[(&str, f64)]) -> BTreeMap<ComponenteId, Watts> {
    pares
        .iter()
        .map(|(k, v)| (ComponenteId::new(*k), Watts(*v)))
        .collect()
}

#[test]
fn acumulador_fecha_o_minuto_ao_virar() {
    let mut a = Acumulador::default();
    assert!(
        a.adicionar(60, &w(&[("cpu", 60.0)]), Duration::from_secs(30))
            .is_none()
    );
    assert!(
        a.adicionar(60, &w(&[("cpu", 60.0)]), Duration::from_secs(30))
            .is_none()
    );
    let r = a
        .adicionar(120, &w(&[("cpu", 120.0)]), Duration::from_secs(30))
        .unwrap();
    assert_eq!(r.minuto, 60);
    assert_eq!(r.energia[&ComponenteId::new("cpu")], WattHora(1.0));
    assert!((r.segundos_ligado - 60.0).abs() < 1e-9);
    assert_eq!(a.esvaziar().unwrap().minuto, 120);
    assert!(a.esvaziar().is_none());
}

#[test]
fn acumulador_com_relogio_voltando_fecha_e_segue() {
    let mut a = Acumulador::default();
    a.adicionar(600, &w(&[("cpu", 60.0)]), Duration::from_secs(2));
    let r = a
        .adicionar(540, &w(&[("cpu", 60.0)]), Duration::from_secs(2))
        .unwrap();
    assert_eq!(r.minuto, 600);
    assert_eq!(a.esvaziar().unwrap().minuto, 540);
}

struct Falso {
    comps: Vec<Componente>,
    amostras: Vec<Amostra>,
    reinicios: Arc<AtomicUsize>,
    quebrar: Arc<AtomicBool>,
}
impl Medidor for Falso {
    fn componentes(&self) -> Vec<Componente> {
        self.comps.clone()
    }
    fn ler(&mut self, _dt: Duration) -> Vec<(ComponenteId, Amostra)> {
        let quebrado = self.quebrar.load(Ordering::SeqCst);
        self.comps
            .iter()
            .zip(&self.amostras)
            .map(|(c, a)| {
                (
                    c.id.clone(),
                    if quebrado && c.perfil == Perfil::Sensor {
                        Amostra::SemLeitura
                    } else {
                        *a
                    },
                )
            })
            .collect()
    }
    fn reiniciar(&mut self) {
        self.reinicios.fetch_add(1, Ordering::SeqCst);
    }
}

type Cenario = (
    Amostrador,
    Arc<RelogioFalso>,
    watch::Receiver<Arc<EstadoAoVivo>>,
    Arc<AtomicUsize>,
    Arc<AtomicBool>,
    watch::Sender<Config>,
);

fn montar() -> Cenario {
    let reinicios = Arc::new(AtomicUsize::new(0));
    let quebrar = Arc::new(AtomicBool::new(false));
    let medidor = Falso {
        comps: vec![
            Componente::novo("gpu", "GPU", "NVML", Perfil::Sensor),
            Componente::novo("disco:a", "HDD A", "atividade do disco", Perfil::Hdd),
            Componente::novo("ram", "RAM", "valor fixo", Perfil::Ram),
        ],
        amostras: vec![
            Amostra::Potencia(Watts(100.0)),
            Amostra::Atividade(0.5),
            Amostra::Fixo,
        ],
        reinicios: Arc::clone(&reinicios),
        quebrar: Arc::clone(&quebrar),
    };
    let relogio = Arc::new(RelogioFalso::novo(hora("2026-10-10T12:00:00")));
    let (cfg_tx, cfg_rx) = watch::channel(Config::padrao(false));
    let (est_tx, est_rx) = watch::channel(Arc::new(EstadoAoVivo::default()));
    let a = Amostrador::novo(vec![Box::new(medidor)], relogio.clone(), cfg_rx, est_tx);
    (a, relogio, est_rx, reinicios, quebrar, cfg_tx)
}

#[test]
fn amostrador_estima_publica_estado_e_fecha_minuto() {
    let (mut a, relogio, est, _, _, _) = montar();
    let mut fechados = Vec::new();
    for _ in 0..30 {
        relogio.avancar(Duration::from_secs(2)); // 12:00:02 … 12:01:00
        fechados.extend(a.passo());
    }
    let e = est.borrow().clone();
    assert_eq!(e.watts[&ComponenteId::new("gpu")], Watts(100.0));
    assert_eq!(e.watts[&ComponenteId::new("disco:a")], Watts(5.0)); // HDD 4..6 W com 50%
    assert_eq!(e.watts[&ComponenteId::new("ram")], Watts(8.0));
    assert_eq!(e.atividade[&ComponenteId::new("disco:a")], 0.5);
    // o 30º passo cai em 12:01:00 e fecha o minuto 12:00 com 29 passos (58 s)
    assert_eq!(fechados.len(), 1);
    let r = &fechados[0];
    assert!((r.segundos_ligado - 58.0).abs() < 1e-9);
    assert!((r.energia[&ComponenteId::new("gpu")].0 - 100.0 * 58.0 / 3600.0).abs() < 1e-9);
}

#[test]
fn amostrador_descarta_intervalo_de_suspensao_e_reinicia_medidores() {
    let (mut a, relogio, _, reinicios, _, _) = montar();
    relogio.avancar(Duration::from_secs(2));
    a.passo();
    relogio.avancar(Duration::from_secs(3600)); // suspenso por 1 h
    assert!(a.passo().is_none());
    assert_eq!(reinicios.load(Ordering::SeqCst), 1);
    let r = a.encerrar().unwrap();
    assert!((r.segundos_ligado - 2.0).abs() < 1e-9); // a hora suspensa não conta
}

#[test]
fn sensor_que_para_vira_faltando_e_conta_zero() {
    let (mut a, relogio, est, _, quebrar, _) = montar();
    quebrar.store(true, Ordering::SeqCst);
    relogio.avancar(Duration::from_secs(2));
    a.passo();
    let e = est.borrow().clone();
    assert_eq!(e.faltando, vec![ComponenteId::new("gpu")]);
    assert_eq!(e.watts[&ComponenteId::new("gpu")], Watts(0.0));
}

#[test]
fn mudanca_de_config_vale_na_proxima_leitura() {
    let (mut a, relogio, est, _, _, cfg) = montar();
    cfg.send_modify(|c| *c = c.com_alteracoes(&[("ram_w".into(), 3.0)]).unwrap());
    relogio.avancar(Duration::from_secs(2));
    a.passo();
    assert_eq!(est.borrow().watts[&ComponenteId::new("ram")], Watts(3.0));
}
