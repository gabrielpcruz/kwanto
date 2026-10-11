//! Dashboard, popup e arquivos estáticos embutidos no binário.
use axum::extract::Path;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/"] // relativo à raiz do crate
struct Web;

fn servir(caminho: &str, cache: &'static str) -> Response {
    match Web::get(caminho) {
        Some(arquivo) => {
            let tipo = mime_guess::from_path(caminho).first_or_octet_stream();
            let cabecalhos = [
                (header::CONTENT_TYPE, tipo.as_ref().to_string()),
                (header::CACHE_CONTROL, cache.to_string()),
            ];
            (cabecalhos, arquivo.data).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({ "erro": "não encontrado" })),
        )
            .into_response(),
    }
}

pub async fn dashboard() -> Response {
    servir("dashboard.html", "no-store")
}

pub async fn mini() -> Response {
    servir("mini.html", "no-store")
}

/// CSS, JS, fontes e ícones: o navegador revalida a cada carregamento (ETag do conteúdo), para
/// pegar a versão nova logo depois de uma atualização; sem mudança, a resposta é 304 sem corpo.
pub async fn estatico(Path(caminho): Path<String>, cabecalhos: HeaderMap) -> Response {
    let caminho = format!("static/{caminho}");
    let Some(arquivo) = Web::get(&caminho) else {
        return servir(&caminho, "no-cache");
    };
    let etag: String = format!(
        "\"{}\"",
        arquivo
            .metadata
            .sha256_hash()
            .iter()
            .take(16)
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let igual = cabecalhos
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag));
    let mut resposta = if igual {
        StatusCode::NOT_MODIFIED.into_response()
    } else {
        servir(&caminho, "no-cache")
    };
    let h = resposta.headers_mut();
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    if let Ok(v) = HeaderValue::from_str(&etag) {
        h.insert(header::ETAG, v);
    }
    resposta
}
