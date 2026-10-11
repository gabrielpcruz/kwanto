//! Versões "maior.menor.correção" (sem sufixos: só versões publicadas).
use crate::ErroAtualizacao;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Versao {
    pub maior: u64,
    pub menor: u64,
    pub correcao: u64,
}

impl Versao {
    pub fn ler(texto: &str) -> Result<Self, ErroAtualizacao> {
        let erro = || ErroAtualizacao::Versao(texto.to_string());
        let texto_limpo = texto.trim();
        let texto_limpo = texto_limpo.strip_prefix('v').unwrap_or(texto_limpo);
        let partes: Vec<u64> = texto_limpo
            .split('.')
            .map(|p| {
                if !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()) {
                    p.parse().ok()
                } else {
                    None
                }
            })
            .collect::<Option<_>>()
            .ok_or_else(erro)?;
        match partes[..] {
            [maior, menor, correcao] => Ok(Self {
                maior,
                menor,
                correcao,
            }),
            _ => Err(erro()),
        }
    }

    /// Versão deste binário.
    #[must_use]
    pub fn atual() -> Self {
        Self::ler(env!("CARGO_PKG_VERSION")).unwrap_or(Self {
            maior: 0,
            menor: 0,
            correcao: 0,
        })
    }
}

impl fmt::Display for Versao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.maior, self.menor, self.correcao)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ler_e_comparar() {
        assert_eq!(
            Versao::ler("v1.2.3").unwrap(),
            Versao {
                maior: 1,
                menor: 2,
                correcao: 3
            }
        );
        assert!(Versao::ler("0.10.0").unwrap() > Versao::ler("0.9.9").unwrap());
        assert!(Versao::ler("1.0.0").unwrap() > Versao::ler("0.99.99").unwrap());
        assert_eq!(Versao::ler("0.2.0").unwrap().to_string(), "0.2.0");
    }

    #[test]
    fn versoes_invalidas() {
        for v in [
            "",
            "1.2",
            "1.2.3.4",
            "a.b.c",
            "1.2.-3",
            "1.2.3-rc1",
            "vv1.2.3",
        ] {
            assert!(Versao::ler(v).is_err(), "{v}");
        }
    }
}
