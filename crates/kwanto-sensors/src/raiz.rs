//! Raiz do sistema de arquivos usada pelos sensores; nos testes aponta para uma árvore falsa.
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct RaizDoSistema(PathBuf);

impl RaizDoSistema {
    #[must_use]
    pub fn real() -> Self {
        Self(PathBuf::from("/"))
    }

    #[must_use]
    pub fn em(base: PathBuf) -> Self {
        Self(base)
    }

    /// Caminho dentro da raiz (aceita "sys/..." ou "/sys/...").
    #[must_use]
    pub fn caminho(&self, relativo: &str) -> PathBuf {
        self.0.join(relativo.trim_start_matches('/'))
    }
}

#[must_use]
pub fn ler_txt(p: &Path) -> Option<String> {
    std::fs::read_to_string(p)
        .ok()
        .map(|s| s.trim().to_string())
}

#[must_use]
pub fn ler_u64(p: &Path) -> Option<u64> {
    ler_txt(p)?.parse().ok()
}
