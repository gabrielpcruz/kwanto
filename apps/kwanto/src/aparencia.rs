//! O que o ícone mostra: rótulo ao lado, ícone ativo/apagado e a dica ao passar o mouse.
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Aparencia {
    pub rotulo: String,
    pub ativo: bool,
    pub dica_titulo: String,
    pub dica_texto: String,
}

/// Valor em reais no formato brasileiro: "R$ 1.234,56".
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn brl(v: f64) -> String {
    let centavos = (v.abs() * 100.0).round() as u64;
    let inteiro = (centavos / 100).to_string();
    let mut com_pontos = String::new();
    for (i, c) in inteiro.chars().enumerate() {
        if i > 0 && (inteiro.len() - i) % 3 == 0 {
            com_pontos.push('.');
        }
        com_pontos.push(c);
    }
    let sinal = if v < 0.0 { "-" } else { "" };
    format!("{sinal}R$ {com_pontos},{:02}", centavos % 100)
}

fn numero(s: &Value, caminho: &[&str]) -> f64 {
    caminho
        .iter()
        .try_fold(s, |v, k| v.get(k))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
}

impl Aparencia {
    #[must_use]
    pub fn de_status(status: Option<&Value>) -> Self {
        let Some(s) = status else {
            return Self {
                rotulo: "– W".into(),
                ativo: false,
                dica_titulo: "Kwanto".into(),
                dica_texto: "O serviço do Kwanto não está rodando.".into(),
            };
        };
        let w = numero(s, &["agora", "parede_w"]);
        Self {
            rotulo: format!("{w:.0} W"),
            ativo: true,
            dica_titulo: "Kwanto".into(),
            dica_texto: format!(
                "{w:.0} W agora, {} por hora\nHoje: {}\nPrevisão da conta: {}",
                brl(numero(s, &["agora", "custo_hora"])),
                brl(numero(s, &["hoje", "custo"])),
                brl(numero(s, &["ciclo", "previsao_custo"])),
            ),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use serde_json::json;

    #[test]
    fn servico_parado() {
        let a = Aparencia::de_status(None);
        assert_eq!(a.rotulo, "– W");
        assert!(!a.ativo);
        assert_eq!(a.dica_texto, "O serviço do Kwanto não está rodando.");
    }

    #[test]
    fn status_normal() {
        let s = json!({
            "agora": {"parede_w": 141.6, "custo_hora": 0.1203},
            "hoje": {"custo": 2.4},
            "ciclo": {"previsao_custo": 1234.5}
        });
        let a = Aparencia::de_status(Some(&s));
        assert_eq!(a.rotulo, "142 W");
        assert!(a.ativo);
        assert_eq!(a.dica_titulo, "Kwanto");
        assert_eq!(
            a.dica_texto,
            "142 W agora, R$ 0,12 por hora\nHoje: R$ 2,40\nPrevisão da conta: R$ 1.234,50"
        );
    }

    #[test]
    fn status_incompleto_nao_entra_em_panico() {
        let a = Aparencia::de_status(Some(&json!({"agora": {}})));
        assert_eq!(a.rotulo, "0 W");
    }

    #[test]
    fn reais_no_formato_brasileiro() {
        assert_eq!(brl(0.0), "R$ 0,00");
        assert_eq!(brl(1234567.891), "R$ 1.234.567,89");
        assert_eq!(brl(-3.5), "-R$ 3,50");
    }
}
