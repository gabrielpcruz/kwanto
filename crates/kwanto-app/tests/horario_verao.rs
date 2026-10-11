//! O Brasil pode voltar a ter horário de verão. O Kwanto segue as regras de fuso do sistema
//! (tzdata), então não há nada para "ativar": estes testes garantem que os cálculos acertam nos
//! dias de mudança, usando as transições reais de São Paulo em 2018/2019.
use jiff::Timestamp;
use jiff::tz::TimeZone;
use kwanto_app::coleta::{Acumulador, EstadoAoVivo};
use kwanto_app::historico::{ConsultarHistorico, Faixa};
use kwanto_app::memoria::RepositorioMemoria;
use kwanto_app::portas::RepositorioEnergia;
use kwanto_app::status::{ConsultarStatus, InfoHardware};
use kwanto_core::componente::ComponenteId;
use kwanto_core::config::Config;
use kwanto_core::eventos::RegistroMinuto;
use kwanto_core::relogio::RelogioFalso;
use kwanto_core::unidades::{WattHora, Watts};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

fn sao_paulo(utc: &str) -> jiff::Zoned {
    utc.parse::<Timestamp>()
        .unwrap()
        .to_zoned(TimeZone::get("America/Sao_Paulo").unwrap())
}

/// Um minuto com 1 Wh de CPU, no instante UTC dado.
fn minuto(repo: &RepositorioMemoria, utc: &str) {
    let r = RegistroMinuto {
        minuto: utc.parse::<Timestamp>().unwrap().as_second() / 60 * 60,
        energia: [(ComponenteId::new("cpu"), WattHora(1.0))].into(),
        segundos_ligado: 60.0,
    };
    repo.gravar_minuto(&r).unwrap();
}

fn eficiencia_total() -> watch::Receiver<Config> {
    // eficiência 1,0: sem perda da fonte, para conferir os Wh exatos
    watch::channel(
        Config::padrao(false)
            .com_alteracoes(&[("eficiencia_fonte".into(), 1.0)])
            .unwrap(),
    )
    .1
}

#[test]
fn hoje_comeca_a_01h_no_dia_em_que_a_meia_noite_nao_existe() {
    // 4/11/2018: às 00:00 (-03) o relógio pulou para 01:00 (-02); o dia começa em 03:00 UTC.
    let repo = Arc::new(RepositorioMemoria::default());
    minuto(&repo, "2018-11-04T02:30:00Z"); // 23:30 de 3/11 → não é "hoje"
    minuto(&repo, "2018-11-04T03:30:00Z"); // 01:30 de 4/11 → é "hoje"
    let relogio = Arc::new(RelogioFalso::novo(sao_paulo("2018-11-04T15:00:00Z")));
    let (_, est) = watch::channel(Arc::new(EstadoAoVivo::default()));
    let info = InfoHardware {
        cpu_estimada: false,
        tem_hdd: false,
    };
    let uc = ConsultarStatus::novo(repo.clone(), repo, relogio, eficiencia_total(), est, info);
    let s = serde_json::to_value(uc.executar().unwrap()).unwrap();
    assert!(
        (s["hoje"]["kwh"].as_f64().unwrap() - 0.001).abs() < 1e-12,
        "{}",
        s["hoje"]
    );
}

#[test]
fn ciclo_que_comeca_no_dia_sem_meia_noite_nao_falha() {
    let repo = Arc::new(RepositorioMemoria::default());
    minuto(&repo, "2018-11-04T03:30:00Z");
    let relogio = Arc::new(RelogioFalso::novo(sao_paulo("2018-11-10T15:00:00Z")));
    let cfg = Config::padrao(false)
        .com_alteracoes(&[("dia_leitura".into(), 4.0)])
        .unwrap();
    let (_, cfg) = watch::channel(cfg);
    let (_, est) = watch::channel(Arc::new(EstadoAoVivo::default()));
    let info = InfoHardware {
        cpu_estimada: false,
        tem_hdd: false,
    };
    let uc = ConsultarStatus::novo(repo.clone(), repo, relogio, cfg, est, info);
    let s = serde_json::to_value(uc.executar().unwrap()).unwrap();
    assert_eq!(s["ciclo"]["inicio"], "2018-11-04");
    assert_eq!(s["ciclo"]["fim"], "2018-12-04");
}

#[test]
fn historico_por_dia_poe_a_hora_repetida_no_dia_certo() {
    // 16/02/2019: à meia-noite (-02) o relógio voltou para 23:00 (-03); as 23h aconteceram duas vezes.
    let repo = Arc::new(RepositorioMemoria::default());
    minuto(&repo, "2019-02-17T01:30:00Z"); // 23:30 (-02) de 16/02, primeira vez
    minuto(&repo, "2019-02-17T02:30:00Z"); // 23:30 (-03) de 16/02, segunda vez
    minuto(&repo, "2019-02-17T03:30:00Z"); // 00:30 (-03) de 17/02
    let relogio = Arc::new(RelogioFalso::novo(sao_paulo("2019-02-17T15:00:00Z")));
    let uc = ConsultarHistorico::novo(repo, relogio, eficiencia_total());
    let h = serde_json::to_value(uc.executar(Faixa::D30).unwrap()).unwrap();
    let dias: Vec<(String, f64)> = h["dias"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| {
            (
                d["dia"].as_str().unwrap().to_string(),
                (d["horas_ligado"].as_f64().unwrap() * 60.0).round(),
            )
        })
        .collect();
    assert_eq!(
        dias,
        vec![
            ("2019-02-16".to_string(), 2.0),
            ("2019-02-17".to_string(), 1.0)
        ]
    );
}

#[test]
fn acumulador_nao_perde_nem_repete_minutos_na_mudanca_de_horario() {
    // atravessa o início do horário de verão de 2018 de 2 em 2 segundos
    let relogio = RelogioFalso::novo(sao_paulo("2018-11-04T02:55:00Z"));
    let watts = [(ComponenteId::new("cpu"), Watts(60.0))].into();
    let mut a = Acumulador::default();
    let mut fechados = Vec::new();
    for _ in 0..(10 * 30) {
        relogio.avancar(Duration::from_secs(2));
        let m = kwanto_core::relogio::Relogio::agora(&relogio)
            .timestamp()
            .as_second()
            / 60
            * 60;
        fechados.extend(a.adicionar(m, &watts, Duration::from_secs(2)));
    }
    let minutos: Vec<i64> = fechados.iter().map(|r| r.minuto).collect();
    assert_eq!(minutos.len(), 10); // 02:55 a 03:04 UTC fechados; 03:05 fica aberto
    assert!(
        minutos.windows(2).all(|p| p[1] - p[0] == 60),
        "pulou ou repetiu minuto: {minutos:?}"
    );
}
