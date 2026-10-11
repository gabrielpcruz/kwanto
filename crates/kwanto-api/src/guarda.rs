//! Proteções de um servidor local: Host fixo (DNS rebinding), JSON obrigatório no POST e Origin conferido.
use axum::extract::{Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

#[derive(Debug, Clone)]
pub struct Permitidos {
    hosts: [String; 2],
    origens: [String; 2],
}

impl Permitidos {
    #[must_use]
    pub fn para_porta(porta: u16) -> Self {
        Self {
            hosts: [format!("127.0.0.1:{porta}"), format!("localhost:{porta}")],
            origens: [
                format!("http://127.0.0.1:{porta}"),
                format!("http://localhost:{porta}"),
            ],
        }
    }
}

fn negar(status: StatusCode, msg: &str) -> Response {
    (status, axum::Json(serde_json::json!({ "erro": msg }))).into_response()
}

pub async fn guarda(State(p): State<Permitidos>, req: Request, next: Next) -> Response {
    // Decide antes de qualquer `await`: segurar `&req` através dele deixaria a future não-`Send`.
    if let Some(negado) = conferir(&p, &req) {
        return negado;
    }
    next.run(req).await
}

fn conferir(p: &Permitidos, req: &Request) -> Option<Response> {
    let cabecalho = |h| req.headers().get(h).and_then(|v| v.to_str().ok());
    let host = cabecalho(header::HOST).unwrap_or_default();
    if !p.hosts.iter().any(|h| h == host) {
        return Some(negar(StatusCode::FORBIDDEN, "host não permitido"));
    }
    if req.method() == Method::POST {
        if !cabecalho(header::CONTENT_TYPE).is_some_and(|t| t.starts_with("application/json")) {
            return Some(negar(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "envie JSON (Content-Type: application/json)",
            ));
        }
        if cabecalho(header::ORIGIN).is_some_and(|o| !p.origens.iter().any(|x| x == o)) {
            return Some(negar(StatusCode::FORBIDDEN, "origem não permitida"));
        }
    }
    None
}
