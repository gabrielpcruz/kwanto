use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use jiff::tz::TimeZone;
use kwanto_api::acesso::{Acesso, Porteiro};
use kwanto_api::{Contexto, EstadoApi, rotas};
use kwanto_app::coleta::EstadoAoVivo;
use kwanto_app::config_uc::AtualizarConfig;
use kwanto_app::historico::ConsultarHistorico;
use kwanto_app::memoria::RepositorioMemoria;
use kwanto_app::status::{ConsultarStatus, InfoHardware};
use kwanto_core::config::Config;
use kwanto_core::relogio::RelogioFalso;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};
use tower::ServiceExt;

const PORTA: u16 = 8787;

/// Porteiro falso: decide pela porta de origem da conexão (cada porta = um tipo de cliente).
#[derive(Debug)]
struct PorteiroFalso;

const CLIENTE_DA_SESSAO: u16 = 50_000;
const OUTRO_USUARIO: u16 = 50_001;
const SEM_LOGIND: u16 = 50_002;

#[async_trait::async_trait]
impl Porteiro for PorteiroFalso {
    async fn decidir(&self, cliente: std::net::SocketAddr) -> Acesso {
        match cliente.port() {
            CLIENTE_DA_SESSAO => Acesso::Permitido,
            OUTRO_USUARIO => Acesso::Negado("outro usuário"),
            SEM_LOGIND => Acesso::Negado("sem logind"),
            _ => Acesso::Negado("desconhecido"),
        }
    }
}

fn app() -> axum::Router {
    let repo = Arc::new(RepositorioMemoria::default());
    let agora = "2026-10-10T12:00:00"
        .parse::<jiff::civil::DateTime>()
        .unwrap()
        .to_zoned(TimeZone::fixed(jiff::tz::offset(-3)))
        .unwrap();
    let relogio = Arc::new(RelogioFalso::novo(agora));
    let (cfg_tx, cfg_rx) = watch::channel(Config::padrao(false));
    let (_est_tx, est_rx) = watch::channel(Arc::new(EstadoAoVivo::default()));
    let (ev, _) = broadcast::channel(8);
    let info = InfoHardware {
        cpu_estimada: false,
        tem_hdd: false,
    };
    let estado = EstadoApi {
        status: Arc::new(ConsultarStatus::novo(
            repo.clone(),
            repo.clone(),
            relogio.clone(),
            cfg_rx.clone(),
            est_rx.clone(),
            info,
        )),
        historico: Arc::new(ConsultarHistorico::novo(
            repo.clone(),
            relogio,
            cfg_rx.clone(),
        )),
        config: Arc::new(AtualizarConfig::novo(repo.clone(), cfg_tx, ev)),
        paineis: Arc::new(vec![]),
    };
    let ctx = Contexto {
        energia: repo.clone(),
        catalogo: repo,
        config: cfg_rx,
        estado: est_rx,
    };
    rotas(estado, PORTA, &[], ctx, Arc::new(PorteiroFalso))
}

fn de(porta: u16) -> axum::extract::ConnectInfo<std::net::SocketAddr> {
    axum::extract::ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], porta)))
}

fn get(uri: &str, host: &str) -> Request<Body> {
    get_de(uri, host, CLIENTE_DA_SESSAO)
}

fn get_de(uri: &str, host: &str, porta: u16) -> Request<Body> {
    let mut r = Request::get(uri)
        .header(header::HOST, host)
        .body(Body::empty())
        .unwrap();
    r.extensions_mut().insert(de(porta));
    r
}

fn post(corpo: &str, tipo: &str, origem: Option<&str>) -> Request<Body> {
    let mut r = Request::post("/api/config")
        .header(header::HOST, "127.0.0.1:8787")
        .header(header::CONTENT_TYPE, tipo);
    if let Some(o) = origem {
        r = r.header(header::ORIGIN, o);
    }
    let mut r = r.body(Body::from(corpo.to_string())).unwrap();
    r.extensions_mut().insert(de(CLIENTE_DA_SESSAO));
    r
}

async fn corpo(r: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&r.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn status_historico_e_paginas() {
    let a = app();
    let r = a
        .clone()
        .oneshot(get("/api/status", "127.0.0.1:8787"))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(corpo(r).await["versao_api"], 1);
    let r = a
        .clone()
        .oneshot(get("/api/historico?faixa=24h", "localhost:8787"))
        .await
        .unwrap();
    assert_eq!(corpo(r).await["passo"], 600);
    for (uri, tipo) in [
        ("/", "text/html"),
        ("/mini", "text/html"),
        ("/static/tema.css", "text/css"),
        ("/static/unifilar.js", "javascript"),
    ] {
        let r = a.clone().oneshot(get(uri, "127.0.0.1:8787")).await.unwrap();
        assert_eq!(r.status(), StatusCode::OK, "{uri}");
        assert!(
            r.headers()[header::CONTENT_TYPE]
                .to_str()
                .unwrap()
                .contains(tipo),
            "{uri}"
        );
    }
    let r = a
        .clone()
        .oneshot(get("/static/../Cargo.toml", "127.0.0.1:8787"))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
    let r = a
        .oneshot(get("/api/extensoes", "127.0.0.1:8787"))
        .await
        .unwrap();
    assert_eq!(corpo(r).await, serde_json::json!([]));
}

#[tokio::test]
async fn host_estranho_e_bloqueado_contra_dns_rebinding() {
    let r = app()
        .oneshot(get("/api/status", "evil.com:8787"))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn post_config_valido_e_invalidos() {
    let a = app();
    let r = a
        .clone()
        .oneshot(post(
            r#"{"preco_kwh": 0.95}"#,
            "application/json",
            Some("http://127.0.0.1:8787"),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(corpo(r).await["preco_kwh"], 0.95);

    let r = a
        .clone()
        .oneshot(post(r#"{"preco_kwh": 1}"#, "text/plain", None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let r = a
        .clone()
        .oneshot(post(
            r#"{"preco_kwh": 1}"#,
            "application/json",
            Some("http://evil.com"),
        ))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    let r = a
        .clone()
        .oneshot(post("não é json", "application/json", None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let r = a
        .clone()
        .oneshot(post(r#"{"eficiencia_fonte": 3}"#, "application/json", None))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        corpo(r).await["erro"],
        "Eficiência da fonte (0–1): fora do intervalo [0.5, 1]"
    );

    let r = a
        .oneshot(get("/api/status", "127.0.0.1:8787"))
        .await
        .unwrap();
    assert_eq!(corpo(r).await["config"]["preco_kwh"], 0.95); // os inválidos não alteraram nada
}

/// Cabeçalhos de segurança em todas as respostas: o CSP impede que um texto injetado vire script,
/// e as demais impedem embutir o painel em outro site e adivinhar tipos de arquivo.
#[tokio::test]
async fn todas_as_respostas_tem_cabecalhos_de_seguranca() {
    use tower::ServiceExt as _;
    for uri in [
        "/",
        "/mini",
        "/static/tema.css",
        "/api/status",
        "/nao-existe",
    ] {
        let r = app().oneshot(get(uri, "127.0.0.1:8787")).await.unwrap();
        let h = r.headers();
        let csp = h
            .get("content-security-policy")
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        for parte in [
            "default-src 'self'",
            "script-src 'self'",
            "frame-ancestors 'none'",
            "base-uri 'none'",
            "object-src 'none'",
        ] {
            assert!(csp.contains(parte), "{uri}: CSP sem {parte}: {csp:?}");
        }
        assert!(
            !csp.contains("script-src 'self' 'unsafe-inline'"),
            "{uri}: script inline liberado"
        );
        assert_eq!(
            h.get("x-content-type-options").map(|v| v.to_str().unwrap()),
            Some("nosniff"),
            "{uri}"
        );
        assert_eq!(
            h.get("x-frame-options").map(|v| v.to_str().unwrap()),
            Some("DENY"),
            "{uri}"
        );
        assert_eq!(
            h.get("referrer-policy").map(|v| v.to_str().unwrap()),
            Some("no-referrer"),
            "{uri}"
        );
    }
}

/// Só o usuário com a sessão ativa (ou o root) usa a API; as páginas continuam abertas, mas sem os
/// dados elas não mostram nada.
#[tokio::test]
async fn api_so_responde_a_sessao_ativa() {
    for uri in ["/api/status", "/api/historico?faixa=24h", "/api/extensoes"] {
        let ok = app()
            .oneshot(get_de(uri, "127.0.0.1:8787", CLIENTE_DA_SESSAO))
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK, "{uri}");
        for porta in [OUTRO_USUARIO, SEM_LOGIND] {
            let r = app()
                .oneshot(get_de(uri, "127.0.0.1:8787", porta))
                .await
                .unwrap();
            assert_eq!(r.status(), StatusCode::FORBIDDEN, "{uri} de {porta}");
            assert!(
                corpo(r).await["erro"]
                    .as_str()
                    .unwrap()
                    .contains("sessão ativa")
            );
        }
    }
    let mut p = post(r#"{"preco_kwh": 1.0}"#, "application/json", None);
    p.extensions_mut().insert(de(OUTRO_USUARIO));
    assert_eq!(
        app().oneshot(p).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let pagina = app()
        .oneshot(get_de("/", "127.0.0.1:8787", OUTRO_USUARIO))
        .await
        .unwrap();
    assert_eq!(pagina.status(), StatusCode::OK);
}

/// Sem saber de onde veio a conexão, nega (falha fechada).
#[tokio::test]
async fn sem_endereco_do_cliente_a_api_nega() {
    let r = Request::get("/api/status")
        .header(header::HOST, "127.0.0.1:8787")
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app().oneshot(r).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

/// Depois de uma atualização, o navegador precisa pegar o CSS/JS novos na hora: os estáticos são
/// revalidados a cada carregamento (ETag), sem guardar cópia velha por um dia.
#[tokio::test]
async fn estaticos_sao_revalidados_depois_de_atualizar() {
    use tower::ServiceExt as _;
    let r = app()
        .oneshot(get("/static/tema.css", "127.0.0.1:8787"))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(r.headers()["cache-control"], "no-cache");
    let etag = r
        .headers()
        .get("etag")
        .expect("sem ETag")
        .to_str()
        .unwrap()
        .to_string();
    let mut de_novo = get("/static/tema.css", "127.0.0.1:8787");
    de_novo
        .headers_mut()
        .insert(header::IF_NONE_MATCH, etag.parse().unwrap());
    assert_eq!(
        app().oneshot(de_novo).await.unwrap().status(),
        StatusCode::NOT_MODIFIED,
        "mesmo arquivo: 304"
    );
    let mut outro = get("/static/tema.css", "127.0.0.1:8787");
    outro
        .headers_mut()
        .insert(header::IF_NONE_MATCH, "\"versao-antiga\"".parse().unwrap());
    assert_eq!(
        app().oneshot(outro).await.unwrap().status(),
        StatusCode::OK,
        "arquivo mudou: entrega o novo"
    );
}
