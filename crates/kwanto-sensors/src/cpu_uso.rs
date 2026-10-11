//! CPU estimada pela fração de tempo ocupado em /proc/stat (quando não há RAPL).
use crate::raiz::{RaizDoSistema, ler_txt};
use kwanto_core::componente::{Amostra, Componente, ComponenteId, Perfil};
use kwanto_core::sensores::Medidor;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug)]
pub struct CpuPorUso {
    arquivo: PathBuf,
    ultimo: Option<(u64, u64)>,
}

impl CpuPorUso {
    #[must_use]
    pub fn novo(raiz: &RaizDoSistema) -> Self {
        let arquivo = raiz.caminho("proc/stat");
        let ultimo = Self::tempos(&arquivo);
        Self { arquivo, ultimo }
    }

    /// (ocioso, total) da linha agregada "cpu".
    fn tempos(arquivo: &Path) -> Option<(u64, u64)> {
        let texto = ler_txt(arquivo)?;
        let linha = texto.lines().next()?;
        let v: Vec<u64> = linha
            .split_whitespace()
            .skip(1)
            .take(8)
            .filter_map(|x| x.parse().ok())
            .collect();
        if v.len() < 4 {
            return None;
        }
        Some((v[3] + v.get(4).copied().unwrap_or(0), v.iter().sum()))
    }
}

impl Medidor for CpuPorUso {
    fn componentes(&self) -> Vec<Componente> {
        vec![Componente::novo("cpu", "CPU", "uso da CPU", Perfil::Cpu)]
    }

    fn ler(&mut self, _dt: Duration) -> Vec<(ComponenteId, Amostra)> {
        let atual = Self::tempos(&self.arquivo);
        let anterior = std::mem::replace(&mut self.ultimo, atual);
        let amostra = match (anterior, atual) {
            (Some((oc0, tot0)), Some((oc1, tot1))) if tot1 > tot0 => {
                #[allow(clippy::cast_precision_loss)]
                let uso = 1.0 - oc1.saturating_sub(oc0) as f64 / (tot1 - tot0) as f64;
                Amostra::Atividade(uso.clamp(0.0, 1.0))
            }
            _ => Amostra::Atividade(0.0),
        };
        vec![(ComponenteId::new("cpu"), amostra)]
    }

    fn reiniciar(&mut self) {
        self.ultimo = Self::tempos(&self.arquivo);
    }
}
