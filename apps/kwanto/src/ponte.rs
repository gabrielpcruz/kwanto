//! Mensagens que a página do popup (mini.html) manda para o programa.
use serde_json::Value;

pub const ALTURA_MIN: u32 = 100;
pub const ALTURA_MAX: u32 = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MensagemPopup {
    /// A página mediu o próprio conteúdo; a janela acompanha.
    Altura(u32),
    /// Abrir o dashboard completo no navegador.
    Abrir,
    Fechar,
    /// Remover o ícone da barra (encerrar o programa).
    Sair,
}

/// Mensagens desconhecidas ou malformadas são ignoradas.
#[must_use]
pub fn interpretar(texto: &str) -> Option<MensagemPopup> {
    let v: Value = serde_json::from_str(texto).ok()?;
    match v.get("acao")?.as_str()? {
        "altura" => {
            let h = v.get("valor")?.as_f64()?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let h = h.clamp(f64::from(ALTURA_MIN), f64::from(ALTURA_MAX)) as u32;
            Some(MensagemPopup::Altura(h))
        }
        "abrir" => Some(MensagemPopup::Abrir),
        "fechar" => Some(MensagemPopup::Fechar),
        "sair" => Some(MensagemPopup::Sair),
        _ => None,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn mensagens_conhecidas() {
        assert_eq!(
            interpretar(r#"{"acao":"altura","valor":412}"#),
            Some(MensagemPopup::Altura(412))
        );
        assert_eq!(
            interpretar(r#"{"acao":"abrir"}"#),
            Some(MensagemPopup::Abrir)
        );
        assert_eq!(
            interpretar(r#"{"acao":"fechar"}"#),
            Some(MensagemPopup::Fechar)
        );
        assert_eq!(interpretar(r#"{"acao":"sair"}"#), Some(MensagemPopup::Sair));
    }

    #[test]
    fn altura_absurda_e_limitada() {
        assert_eq!(
            interpretar(r#"{"acao":"altura","valor":5}"#),
            Some(MensagemPopup::Altura(ALTURA_MIN))
        );
        assert_eq!(
            interpretar(r#"{"acao":"altura","valor":99999}"#),
            Some(MensagemPopup::Altura(ALTURA_MAX))
        );
        assert_eq!(
            interpretar(r#"{"acao":"altura","valor":-3}"#),
            Some(MensagemPopup::Altura(ALTURA_MIN))
        );
    }

    #[test]
    fn lixo_e_ignorado() {
        assert_eq!(interpretar("não é json"), None);
        assert_eq!(interpretar(r#"{"acao":"formatar_disco"}"#), None);
        assert_eq!(interpretar(r#"{"acao":"altura","valor":"alto"}"#), None);
        assert_eq!(interpretar("[]"), None);
    }
}
