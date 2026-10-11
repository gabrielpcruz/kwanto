//! API HTTP e dashboard embutido do Kwanto. Os handlers só adaptam: o trabalho está nos casos de uso.
pub mod acesso;
mod erros;
mod estaticos;
mod guarda;

use axum::extract::{Query, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use erros::ErroApi;
use kwanto_app::coleta::EstadoAoVivo;
use kwanto_app::config_uc::{AtualizarConfig, config_como_json};
use kwanto_app::historico::{ConsultarHistorico, Faixa, Historico};
use kwanto_app::portas::{CatalogoComponentes, RepositorioEnergia};
use kwanto_app::status::{ConsultarStatus, Status};
use kwanto_core::config::Config;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::sync::Arc;
use tokio::sync::watch;
use tower_http::set_header::SetResponseHeaderLayer;

/// O que uma extensão HTTP pode usar.
#[derive(Clone)]
pub struct Contexto {
    pub energia: Arc<dyn RepositorioEnergia>,
    pub catalogo: Arc<dyn CatalogoComponentes>,
    pub config: watch::Receiver<Config>,
    pub estado: watch::Receiver<Arc<EstadoAoVivo>>,
}

impl std::fmt::Debug for Contexto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Contexto")
    }
}

/// Painel que uma extensão acrescenta ao dashboard (listado em /api/extensoes).
#[derive(Debug, Clone, Serialize)]
pub struct ManifestoPainel {
    pub id: String,
    pub titulo: String,
    pub url: String,
}

pub trait ExtensaoHttp: Send + Sync {
    /// Rotas próprias; recomenda-se o prefixo `/api/<extensao>/`. Nenhuma rota pode expor energia
    /// de CPU com resolução menor que 1 s (mitigação CVE-2020-8694).
    fn rotas(&self, ctx: Contexto) -> Router;
    fn paineis(&self) -> Vec<ManifestoPainel> {
        Vec::new()
    }
}

#[derive(Clone)]
pub struct EstadoApi {
    pub status: Arc<ConsultarStatus>,
    pub historico: Arc<ConsultarHistorico>,
    pub config: Arc<AtualizarConfig>,
    pub paineis: Arc<Vec<ManifestoPainel>>,
}

impl std::fmt::Debug for EstadoApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EstadoApi")
    }
}

/// Roda um caso de uso síncrono (banco) fora das threads do tokio.
async fn bloqueante<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ErroApi> + Send + 'static,
) -> Result<T, ErroApi> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ErroApi(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
}

async fn status(State(e): State<EstadoApi>) -> Result<Json<Status>, ErroApi> {
    bloqueante(move || Ok(e.status.executar()?)).await.map(Json)
}

#[derive(Debug, Deserialize)]
struct ParamHistorico {
    faixa: Option<String>,
}

async fn historico(
    State(e): State<EstadoApi>,
    Query(p): Query<ParamHistorico>,
) -> Result<Json<Historico>, ErroApi> {
    let faixa = Faixa::de_texto(p.faixa.as_deref().unwrap_or("24h"));
    bloqueante(move || Ok(e.historico.executar(faixa)?))
        .await
        .map(Json)
}

async fn atualizar_config(
    State(e): State<EstadoApi>,
    corpo: axum::body::Bytes,
) -> Result<Json<Map<String, Value>>, ErroApi> {
    let invalido = |m: &str| ErroApi(StatusCode::BAD_REQUEST, m.into());
    let valor: Value =
        serde_json::from_slice(&corpo).map_err(|_| invalido("corpo não é JSON válido"))?;
    let mapa = valor
        .as_object()
        .cloned()
        .ok_or_else(|| invalido("envie um objeto JSON"))?;
    let nova = bloqueante(move || Ok(e.config.executar(&mapa)?)).await?;
    Ok(Json(config_como_json(&nova)))
}

async fn extensoes(State(e): State<EstadoApi>) -> Json<Vec<ManifestoPainel>> {
    Json(e.paineis.as_ref().clone())
}

pub fn rotas(
    estado: EstadoApi,
    porta: u16,
    extensoes_http: &[Arc<dyn ExtensaoHttp>],
    ctx: Contexto,
    porteiro: Arc<dyn acesso::Porteiro>,
) -> Router {
    let api = Router::new()
        .route("/api/status", get(status))
        .route("/api/historico", get(historico))
        .route("/api/config", post(atualizar_config))
        .route("/api/extensoes", get(extensoes))
        .with_state(estado)
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ));
    let mut app = Router::new()
        .route("/", get(estaticos::dashboard))
        .route("/mini", get(estaticos::mini))
        .route("/static/{*caminho}", get(estaticos::estatico))
        .merge(api);
    for e in extensoes_http {
        app = app.merge(e.rotas(ctx.clone()));
    }
    // Ordem (de fora para dentro): Host/Origin, depois quem está chamando.
    let app = app
        .layer(axum::middleware::from_fn_with_state(
            porteiro,
            acesso::so_sessao_ativa,
        ))
        .layer(axum::middleware::from_fn_with_state(
            guarda::Permitidos::para_porta(porta),
            guarda::guarda,
        ));
    // Por fora de tudo, para valer também nas respostas de erro.
    CABECALHOS_SEGURANCA.iter().fold(app, |app, (nome, valor)| {
        app.layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static(nome),
            HeaderValue::from_static(valor),
        ))
    })
}

/// Só scripts e estilos servidos pelo próprio Kwanto; um texto injetado nos dados nunca vira
/// script. Estilos inline ficam liberados (barras de porcentagem usam `style=`).
const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
    img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-src 'self'; object-src 'none'; \
    frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

const CABECALHOS_SEGURANCA: [(&str, &str); 4] = [
    ("content-security-policy", CSP),
    ("x-content-type-options", "nosniff"),
    ("x-frame-options", "DENY"),
    ("referrer-policy", "no-referrer"),
];
