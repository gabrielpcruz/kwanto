//! Erros da API como JSON `{"erro": "..."}`.
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use kwanto_app::config_uc::ErroAtualizar;
use kwanto_app::portas::ErroRepositorio;

#[derive(Debug)]
pub struct ErroApi(pub StatusCode, pub String);

impl IntoResponse for ErroApi {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "erro": self.1 }))).into_response()
    }
}

impl From<ErroRepositorio> for ErroApi {
    fn from(e: ErroRepositorio) -> Self {
        tracing::error!(erro = %e, "falha ao consultar o banco");
        Self(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}

impl From<ErroAtualizar> for ErroApi {
    fn from(e: ErroAtualizar) -> Self {
        match e {
            ErroAtualizar::Invalido(m) => Self(StatusCode::BAD_REQUEST, m),
            ErroAtualizar::Armazenamento(e) => e.into(),
        }
    }
}
