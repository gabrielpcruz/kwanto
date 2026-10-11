//! Quem pode usar a API: a API revela a rotina de uso do PC (histórico por minuto) e permite mudar
//! a configuração, então só o usuário com a sessão ativa na máquina (ou o root) a usa. As páginas
//! estáticas continuam abertas: sem os dados, elas não mostram nada.
use axum::extract::{ConnectInfo, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acesso {
    Permitido,
    /// Motivo, só para o log (a resposta é sempre a mesma).
    Negado(&'static str),
}

/// Decide pelo endereço de origem da conexão local.
#[async_trait::async_trait]
pub trait Porteiro: Send + Sync + std::fmt::Debug {
    async fn decidir(&self, cliente: SocketAddr) -> Acesso;
}

/// Mensagem única para qualquer recusa (não revela o motivo a quem foi recusado).
pub const MENSAGEM_NEGADO: &str =
    "o Kwanto só responde ao usuário com a sessão ativa nesta máquina";

pub async fn so_sessao_ativa(
    State(porteiro): State<Arc<dyn Porteiro>>,
    req: Request,
    next: Next,
) -> Response {
    if !req.uri().path().starts_with("/api/") {
        return next.run(req).await;
    }
    let decisao = match req.extensions().get::<ConnectInfo<SocketAddr>>() {
        Some(ConnectInfo(cliente)) => porteiro.decidir(*cliente).await,
        None => Acesso::Negado("conexão sem endereço de origem"),
    };
    match decisao {
        Acesso::Permitido => next.run(req).await,
        Acesso::Negado(motivo) => {
            tracing::warn!(motivo, caminho = %req.uri().path(), "acesso à API recusado");
            (
                StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({ "erro": MENSAGEM_NEGADO })),
            )
                .into_response()
        }
    }
}
