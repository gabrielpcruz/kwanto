use jiff::Zoned;
use jiff::tz::TimeZone;
use kwanto_app::coleta::EstadoAoVivo;
use kwanto_app::historico::{ConsultarHistorico, Faixa};
use kwanto_app::memoria::RepositorioMemoria;
use kwanto_app::portas::{CatalogoComponentes, RepositorioEnergia};
use kwanto_app::status::{ConsultarStatus, InfoHardware};
use kwanto_core::componente::{ComponenteId, DescricaoComponente, TipoComponente};
use kwanto_core::config::Config;
use kwanto_core::eventos::RegistroMinuto;
use kwanto_core::relogio::RelogioFalso;
use kwanto_core::unidades::{WattHora, Watts};
use std::sync::Arc;
use tokio::sync::watch;

fn agora() -> Zoned {
    "2026-10-10T15:00:30"
        .parse::<jiff::civil::DateTime>()
        .unwrap()
        .to_zoned(TimeZone::fixed(jiff::tz::offset(-3)))
        .unwrap()
}

type Cenario = (
    Arc<RepositorioMemoria>,
    Arc<RelogioFalso>,
    watch::Receiver<Config>,
    watch::Receiver<Arc<EstadoAoVivo>>,
);

fn cenario() -> Cenario {
    let repo = Arc::new(RepositorioMemoria::default());
    let d = |id: &str, r: &str, t, o: &str| DescricaoComponente {
        id: ComponenteId::new(id),
        rotulo: r.into(),
        tipo: t,
        origem: o.into(),
    };
    repo.registrar(&[
        d(
            "gpu:GPU-1",
            "NVIDIA GeForce RTX 4060",
            TipoComponente::Medido,
            "NVML (driver NVIDIA)",
        ),
        d("ram", "RAM", TipoComponente::Estimado, "valor fixo"),
    ])
    .unwrap();
    let base = agora().timestamp().as_second() / 60 * 60;
    // 2 h ligado antes de agora: GPU 88 W e RAM 8 W
    for i in 1..=120 {
        let r = RegistroMinuto {
            minuto: base - i * 60,
            energia: [
                (ComponenteId::new("gpu:GPU-1"), WattHora(88.0 / 60.0)),
                (ComponenteId::new("ram"), WattHora(8.0 / 60.0)),
            ]
            .into(),
            segundos_ligado: 60.0,
        };
        repo.gravar_minuto(&r).unwrap();
    }
    let relogio = Arc::new(RelogioFalso::novo(agora()));
    let (_, cfg) = watch::channel(Config::padrao(false));
    let estado = EstadoAoVivo {
        instante: Some(agora().timestamp() - jiff::SignedDuration::from_secs(1)),
        watts: [
            (ComponenteId::new("gpu:GPU-1"), Watts(88.0)),
            (ComponenteId::new("ram"), Watts(8.0)),
        ]
        .into(),
        atividade: Default::default(),
        faltando: vec![],
    };
    let (_, est) = watch::channel(Arc::new(estado));
    (repo, relogio, cfg, est)
}

#[test]
fn status_tem_o_contrato_da_versao_python() {
    let (repo, relogio, cfg, est) = cenario();
    let info = InfoHardware {
        cpu_estimada: true,
        tem_hdd: false,
    };
    let uc = ConsultarStatus::novo(repo.clone(), repo, relogio, cfg, est, info);
    let s = serde_json::to_value(uc.executar().unwrap()).unwrap();
    let chaves: Vec<&str> = s.as_object().unwrap().keys().map(String::as_str).collect();
    for k in [
        "agora",
        "hoje",
        "ciclo",
        "media",
        "estimativa_24x7",
        "config",
        "config_schema",
        "cpu_estimada",
        "tarifa_efetiva",
        "tem_hdd",
        "avisos",
        "versao_api",
    ] {
        assert!(chaves.contains(&k), "faltou {k}");
    }
    // parede = (88 + 8) / 0,88 = 109,09 W; perda = 13,09 W; componente calculado incluído
    assert_eq!(
        s["agora"]["parede_w"].as_f64().unwrap(),
        109.1,
        "arredondado a 0,1 W"
    );
    let comps = s["agora"]["componentes"].as_array().unwrap();
    assert_eq!(comps[0]["id"], "gpu:GPU-1");
    assert!(
        comps
            .iter()
            .any(|c| c["id"] == "fonte_perda" && c["tipo"] == "calculado")
    );
    assert!((s["agora"]["atualizado_ha_s"].as_f64().unwrap() - 1.0).abs() < 1e-9);
    assert_eq!(s["ciclo"]["inicio"], "2026-10-01");
    assert_eq!(s["ciclo"]["fim"], "2026-11-01");
    assert_eq!(s["config"]["dia_leitura"], 1);
    assert_eq!(s["avisos"].as_array().unwrap().len(), 1); // CPU estimada
    insta::assert_json_snapshot!(s);
}

#[test]
fn status_avisa_componente_sem_leitura() {
    let (repo, relogio, cfg, _) = cenario();
    let faltando = EstadoAoVivo {
        faltando: vec![ComponenteId::new("gpu:GPU-1")],
        ..Default::default()
    };
    let (_, est) = watch::channel(Arc::new(faltando));
    let info = InfoHardware {
        cpu_estimada: false,
        tem_hdd: true,
    };
    let uc = ConsultarStatus::novo(repo.clone(), repo, relogio, cfg, est, info);
    let s = serde_json::to_value(uc.executar().unwrap()).unwrap();
    assert_eq!(
        s["avisos"][0],
        "Sem leitura no momento: NVIDIA GeForce RTX 4060. Esses itens contam como 0 W até o sensor voltar."
    );
    assert!(s["agora"]["atualizado_ha_s"].is_null());
}

#[test]
fn historico_24h_7d_e_30d() {
    let (repo, relogio, cfg, _) = cenario();
    let uc = ConsultarHistorico::novo(repo, relogio, cfg);
    let h = serde_json::to_value(uc.executar(Faixa::H24).unwrap()).unwrap();
    assert_eq!(h["passo"], 600);
    let p = h["pontos"].as_array().unwrap();
    assert_eq!(p.len(), 12); // 13:00 a 14:59 em baldes de 10 min
    assert_eq!(p[1]["w"].as_f64().unwrap(), 109.1, "arredondado a 0,1 W");
    let d = serde_json::to_value(uc.executar(Faixa::de_texto("30d")).unwrap()).unwrap();
    let dias = d["dias"].as_array().unwrap();
    assert_eq!(dias.len(), 1);
    assert_eq!(dias[0]["dia"], "2026-10-10");
    assert!((dias[0]["horas_ligado"].as_f64().unwrap() - 2.0).abs() < 1e-9);
    assert!(
        serde_json::to_value(uc.executar(Faixa::de_texto("xyz")).unwrap()).unwrap()["dias"]
            .is_array()
    );
}

/// Conta as consultas ao banco, para provar o cache.
struct Contador {
    repo: Arc<RepositorioMemoria>,
    consultas: std::sync::atomic::AtomicUsize,
}
impl RepositorioEnergia for Contador {
    fn gravar_minuto(&self, r: &RegistroMinuto) -> Result<(), kwanto_app::portas::ErroRepositorio> {
        self.repo.gravar_minuto(r)
    }
    fn totais_por_componente(
        &self,
        i: kwanto_app::portas::Intervalo,
    ) -> Result<
        std::collections::BTreeMap<ComponenteId, WattHora>,
        kwanto_app::portas::ErroRepositorio,
    > {
        self.consultas
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.repo.totais_por_componente(i)
    }
    fn segundos_ligado(
        &self,
        i: kwanto_app::portas::Intervalo,
    ) -> Result<f64, kwanto_app::portas::ErroRepositorio> {
        self.repo.segundos_ligado(i)
    }
    fn primeiro_minuto(&self) -> Result<Option<i64>, kwanto_app::portas::ErroRepositorio> {
        self.repo.primeiro_minuto()
    }
    fn baldes(
        &self,
        i: kwanto_app::portas::Intervalo,
        passo: i64,
    ) -> Result<Vec<kwanto_app::portas::Balde>, kwanto_app::portas::ErroRepositorio> {
        self.repo.baldes(i, passo)
    }
}

#[test]
fn status_consulta_o_banco_uma_vez_por_minuto_e_aplica_ajustes_na_hora() {
    use std::sync::atomic::Ordering;
    let (repo, relogio, _, est) = cenario();
    let contador = Arc::new(Contador {
        repo: repo.clone(),
        consultas: Default::default(),
    });
    let (cfg_tx, cfg) = watch::channel(Config::padrao(false));
    let info = InfoHardware {
        cpu_estimada: false,
        tem_hdd: false,
    };
    let uc = ConsultarStatus::novo(contador.clone(), repo, relogio.clone(), cfg, est, info);

    let primeiro = serde_json::to_value(uc.executar().unwrap()).unwrap();
    let depois_da_primeira = contador.consultas.load(Ordering::SeqCst);
    relogio.avancar(std::time::Duration::from_secs(2));
    uc.executar().unwrap();
    assert_eq!(
        contador.consultas.load(Ordering::SeqCst),
        depois_da_primeira,
        "consultou o banco de novo no mesmo minuto"
    );

    // mudar a eficiência vale na hora, mesmo com as somas em cache
    cfg_tx.send_modify(|c| {
        *c = c
            .com_alteracoes(&[("eficiencia_fonte".into(), 1.0)])
            .unwrap()
    });
    let ajustado = serde_json::to_value(uc.executar().unwrap()).unwrap();
    assert!(ajustado["hoje"]["kwh"].as_f64().unwrap() < primeiro["hoje"]["kwh"].as_f64().unwrap());

    relogio.avancar(std::time::Duration::from_secs(60));
    uc.executar().unwrap();
    assert!(
        contador.consultas.load(Ordering::SeqCst) > depois_da_primeira,
        "não renovou no minuto seguinte"
    );
}

/// Rótulo hostil gravado no catálogo antes da limpeza na entrada (banco antigo, ou dispositivo com
/// nome forjado): a API nunca devolve caracteres com significado em HTML.
#[test]
fn status_nunca_devolve_rotulo_com_html() {
    let (repo, relogio, cfg, _) = cenario();
    repo.registrar(&[DescricaoComponente {
        id: ComponenteId::new("gpu:GPU-1"),
        rotulo: "GPU <img src=x onerror=alert(1)>".into(),
        tipo: TipoComponente::Medido,
        origem: "NVML \"x\" <script>".into(),
    }])
    .unwrap();
    let faltando = EstadoAoVivo {
        faltando: vec![ComponenteId::new("gpu:GPU-1")],
        ..Default::default()
    };
    let (_, est) = watch::channel(Arc::new(faltando));
    let info = InfoHardware {
        cpu_estimada: false,
        tem_hdd: false,
    };
    let s = serde_json::to_string(
        &ConsultarStatus::novo(repo.clone(), repo, relogio, cfg, est, info)
            .executar()
            .unwrap(),
    )
    .unwrap();
    assert!(s.contains("GPU img src=x onerror=alert(1)"), "{s}");
    for proibido in ["<img", "<script", "\\\"x\\\""] {
        assert!(!s.contains(proibido), "{proibido} em {s}");
    }
}

/// A API mostra valores arredondados (0,1 W; 0,001 kWh; R$ 0,01): o que interessa ao usuário, sem a
/// precisão fina que serviria de canal lateral do contador de energia. E nunca "-0.0".
#[test]
fn status_e_historico_saem_arredondados() {
    let (repo, relogio, cfg, est) = cenario();
    let info = InfoHardware {
        cpu_estimada: false,
        tem_hdd: false,
    };
    let s = serde_json::to_value(
        ConsultarStatus::novo(
            repo.clone(),
            repo.clone(),
            relogio.clone(),
            cfg.clone(),
            est,
            info,
        )
        .executar()
        .unwrap(),
    )
    .unwrap();
    let casas = |v: &serde_json::Value, n: i32| {
        let x = v.as_f64().unwrap();
        ((x * 10f64.powi(n)).round() - x * 10f64.powi(n)).abs() < 1e-6
    };
    for c in s["agora"]["componentes"].as_array().unwrap() {
        assert!(casas(&c["w"], 1) && casas(&c["pct"], 1), "{c}");
    }
    for p in ["hoje", "ciclo"] {
        assert!(
            casas(&s[p]["kwh"], 3) && casas(&s[p]["custo"], 2),
            "{p}: {}",
            s[p]
        );
        for c in s[p]["componentes"].as_array().unwrap() {
            assert!(
                casas(&c["kwh"], 3) && casas(&c["custo"], 2) && casas(&c["pct"], 1),
                "{c}"
            );
        }
    }
    assert!(casas(&s["agora"]["parede_w"], 1));
    assert!(
        !serde_json::to_string(&s).unwrap().contains("-0.0"),
        "zero negativo"
    );
    let h = serde_json::to_value(
        ConsultarHistorico::novo(repo, relogio, cfg)
            .executar(Faixa::H24)
            .unwrap(),
    )
    .unwrap();
    for p in h["pontos"].as_array().unwrap() {
        assert!(casas(&p["w"], 1) && casas(&p["kwh"], 3), "{p}");
    }
}
