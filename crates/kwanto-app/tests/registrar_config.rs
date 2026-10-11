use kwanto_app::config_uc::{AtualizarConfig, ErroAtualizar, carregar_config, config_como_json};
use kwanto_app::memoria::RepositorioMemoria;
use kwanto_app::portas::{Intervalo, RepositorioConfig, RepositorioEnergia};
use kwanto_app::registrar::RegistrarMinuto;
use kwanto_core::componente::ComponenteId;
use kwanto_core::config::Config;
use kwanto_core::eventos::{EventoDeDominio, RegistroMinuto};
use kwanto_core::unidades::WattHora;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};

fn reg(minuto: i64) -> RegistroMinuto {
    RegistroMinuto {
        minuto,
        energia: [(ComponenteId::new("cpu"), WattHora(1.0))].into(),
        segundos_ligado: 60.0,
    }
}

#[test]
fn banco_indisponivel_guarda_pendentes_e_regrava_depois() {
    let repo = Arc::new(RepositorioMemoria::default());
    let (tx, mut rx) = broadcast::channel(8);
    let mut r = RegistrarMinuto::novo(repo.clone(), tx);
    repo.falhar_gravacoes(true);
    r.executar(reg(60));
    r.executar(reg(120));
    assert_eq!(r.pendentes(), 2);
    repo.falhar_gravacoes(false);
    r.executar(reg(180));
    assert_eq!(r.pendentes(), 0);
    let total = repo
        .totais_por_componente(Intervalo {
            inicio: 0,
            fim: 1000,
        })
        .unwrap();
    assert_eq!(total[&ComponenteId::new("cpu")], WattHora(3.0));
    let mut eventos = 0;
    while let Ok(EventoDeDominio::MinutoFechado(_)) = rx.try_recv() {
        eventos += 1;
    }
    assert_eq!(eventos, 3);
}

#[test]
fn carregar_config_grava_padroes_na_primeira_vez() {
    let repo = RepositorioMemoria::default();
    let c = carregar_config(&repo, true).unwrap();
    assert_eq!(c.valor("ram_w"), Some(1.5));
    assert_eq!(repo.carregar().unwrap().len(), 14);
}

fn uc() -> (
    AtualizarConfig,
    Arc<RepositorioMemoria>,
    watch::Receiver<Config>,
) {
    let repo = Arc::new(RepositorioMemoria::default());
    let (tx, rx) = watch::channel(Config::padrao(false));
    let (ev, _) = broadcast::channel(8);
    (AtualizarConfig::novo(repo.clone(), tx, ev), repo, rx)
}

#[test]
fn atualizar_aceita_numero_e_texto_numerico_e_publica() {
    let (uc, repo, rx) = uc();
    let corpo = json!({"preco_kwh": 0.95, "ram_w": "4,5", "desconhecida": 3});
    let c = uc.executar(corpo.as_object().unwrap()).unwrap();
    assert_eq!(c.valor("preco_kwh"), Some(0.95));
    assert_eq!(c.valor("ram_w"), Some(4.5));
    assert_eq!(rx.borrow().valor("preco_kwh"), Some(0.95));
    assert!(
        repo.carregar()
            .unwrap()
            .contains(&("preco_kwh".to_string(), 0.95))
    );
}

#[test]
fn atualizar_rejeita_invalidos_sem_alterar_nada() {
    let (uc, repo, rx) = uc();
    for corpo in [
        json!({"preco_kwh": "abc"}),
        json!({"eficiencia_fonte": 3}),
        json!({"dia_leitura": 2.5}),
        json!({"ram_w": null}),
    ] {
        assert!(
            matches!(
                uc.executar(corpo.as_object().unwrap()),
                Err(ErroAtualizar::Invalido(_))
            ),
            "{corpo}"
        );
    }
    assert_eq!(*rx.borrow(), Config::padrao(false));
    assert!(repo.carregar().unwrap().is_empty());
}

#[test]
fn config_em_json_tem_dia_leitura_inteiro() {
    let j = config_como_json(&Config::padrao(false));
    assert_eq!(j["dia_leitura"], json!(1));
    assert_eq!(j["preco_kwh"], json!(0.85));
}
