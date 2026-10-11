//! Conteúdo do popup, embutido no próprio binário e servido pelo protocolo `kwanto://app/`. O popup
//! não carrega nada da rede: os dados chegam pelo monitor, que confere quem é o serviço.

use crate::monitor::UltimoStatus;
use std::borrow::Cow;

/// Origem do popup. Navegação e mensagens IPC só valem dela.
pub const ORIGEM: &str = "kwanto://app/";
pub const PAGINA: &str = "kwanto://app/mini.html";

/// Só o necessário para o popup: a página, o tema, as fontes e os scripts.
#[derive(rust_embed::RustEmbed)]
#[folder = "../../web/"]
#[include = "mini.html"]
#[include = "static/tema.css"]
#[include = "static/unifilar.js"]
#[include = "static/mini.js"]
#[include = "static/fonts/*"]
struct Embutidos;

#[derive(Debug)]
pub struct Resposta {
    pub status: u16,
    pub tipo: &'static str,
    pub corpo: Cow<'static, [u8]>,
}

fn resposta(status: u16, tipo: &'static str, corpo: impl Into<Cow<'static, [u8]>>) -> Resposta {
    Resposta {
        status,
        tipo,
        corpo: corpo.into(),
    }
}

fn tipo_de(caminho: &str) -> &'static str {
    match caminho.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css",
        Some("js") => "text/javascript",
        Some("woff2") => "font/woff2",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

/// Resposta do protocolo `kwanto://app` para um caminho.
#[must_use]
pub fn responder(caminho: &str, ultimo: &UltimoStatus) -> Resposta {
    match caminho {
        "/api/status" => match ultimo.lock().ok().and_then(|u| u.clone()) {
            Some(json) => resposta(200, "application/json", json.into_bytes()),
            None => resposta(503, "text/plain", &b"servico fora do ar"[..]),
        },
        _ => {
            let arquivo = if caminho == "/" {
                "mini.html"
            } else {
                caminho.trim_start_matches('/')
            };
            match Embutidos::get(arquivo) {
                Some(a) if !arquivo.contains("..") => resposta(200, tipo_de(arquivo), a.data),
                _ => resposta(404, "text/plain", &b"nao encontrado"[..]),
            }
        }
    }
}

/// Endereço dentro do próprio popup (`kwanto://app/...`).
#[must_use]
pub fn da_propria_origem(url: &str) -> bool {
    url.starts_with(ORIGEM)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::monitor::UltimoStatus;

    #[test]
    fn serve_a_pagina_e_os_arquivos_embutidos() {
        let u = UltimoStatus::default();
        for caminho in ["/", "/mini.html"] {
            let r = responder(caminho, &u);
            assert_eq!(
                (r.status, r.tipo),
                (200, "text/html; charset=utf-8"),
                "{caminho}"
            );
            assert!(String::from_utf8_lossy(&r.corpo).contains("id=\"cartao\""));
        }
        for caminho in ["/static/tema.css", "/static/unifilar.js", "/static/mini.js"] {
            assert_eq!(responder(caminho, &u).status, 200, "{caminho}");
        }
    }

    #[test]
    fn status_so_o_verificado_pelo_monitor() {
        let u = UltimoStatus::default();
        assert_eq!(
            responder("/api/status", &u).status,
            503,
            "sem status verificado"
        );
        *u.lock().unwrap() = Some(r#"{"agora":{}}"#.into());
        let r = responder("/api/status", &u);
        assert_eq!((r.status, r.tipo), (200, "application/json"));
        assert_eq!(&*r.corpo, br#"{"agora":{}}"#);
    }

    #[test]
    fn nada_alem_do_embutido() {
        let u = UltimoStatus::default();
        for caminho in [
            "/dashboard.html",
            "/static/../../Cargo.toml",
            "/api/config",
            "/etc/passwd",
            "//x",
        ] {
            assert_eq!(responder(caminho, &u).status, 404, "{caminho}");
        }
    }

    #[test]
    fn so_navega_e_so_ouve_a_propria_origem() {
        for ok in ["kwanto://app/", "kwanto://app/mini.html"] {
            assert!(da_propria_origem(ok), "{ok}");
        }
        for nao in [
            "http://127.0.0.1:8787/mini",
            "kwanto://outro/x",
            "kwanto://app.evil/x",
            "file:///etc/passwd",
            "about:blank",
            "javascript:alert(1)",
            "",
        ] {
            assert!(!da_propria_origem(nao), "{nao}");
        }
    }
}
