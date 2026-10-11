//! Preferências do atualizador (por usuário): verificar ou não, e qual versão pular.
use kwanto_atualizacao::versao::Versao;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferencias {
    pub verificar: bool,
    pub pular: Option<String>,
}

impl Default for Preferencias {
    fn default() -> Self {
        Self {
            verificar: true,
            pular: None,
        }
    }
}

impl Preferencias {
    /// `$XDG_CONFIG_HOME/kwanto/atualizacao.json` (ou `~/.config/...`).
    #[must_use]
    pub fn caminho_padrao() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            });
        base.join("kwanto/atualizacao.json")
    }

    /// Ausente ou corrompido vira o padrão.
    #[must_use]
    pub fn carregar(caminho: &Path) -> Self {
        std::fs::read(caminho)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn salvar(&self, caminho: &Path) -> std::io::Result<()> {
        if let Some(pasta) = caminho.parent() {
            std::fs::create_dir_all(pasta)?;
        }
        std::fs::write(caminho, serde_json::to_vec_pretty(self).unwrap_or_default())
    }

    #[must_use]
    pub fn deve_oferecer(&self, versao: &Versao) -> bool {
        self.verificar && self.pular.as_deref() != Some(versao.to_string().as_str())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn v(t: &str) -> Versao {
        Versao::ler(t).unwrap()
    }

    #[test]
    fn padrao_oferece_e_pular_esconde_so_aquela_versao() {
        let p = Preferencias::default();
        assert!(p.deve_oferecer(&v("0.2.0")));
        let p = Preferencias {
            verificar: true,
            pular: Some("0.2.0".into()),
        };
        assert!(!p.deve_oferecer(&v("0.2.0")));
        assert!(p.deve_oferecer(&v("0.3.0")));
        let p = Preferencias {
            verificar: false,
            pular: None,
        };
        assert!(!p.deve_oferecer(&v("9.0.0")));
    }

    #[test]
    fn salvar_carregar_e_arquivo_corrompido() {
        let d = tempfile::tempdir().unwrap();
        let c = d.path().join("kwanto/atualizacao.json");
        assert_eq!(Preferencias::carregar(&c), Preferencias::default());
        Preferencias {
            verificar: true,
            pular: Some("0.2.0".into()),
        }
        .salvar(&c)
        .unwrap();
        assert_eq!(Preferencias::carregar(&c).pular.as_deref(), Some("0.2.0"));
        std::fs::write(&c, "{lixo").unwrap();
        assert_eq!(Preferencias::carregar(&c), Preferencias::default());
    }
}
