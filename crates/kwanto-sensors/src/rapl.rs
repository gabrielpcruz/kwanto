//! CPU medida pelo RAPL do kernel (Intel e AMD), somando todos os pacotes.
use crate::contador::Contador;
use crate::raiz::{RaizDoSistema, ler_txt, ler_u64};
use kwanto_core::componente::{Amostra, Componente, ComponenteId, Perfil};
use kwanto_core::sensores::Medidor;
use kwanto_core::unidades::Watts;
use std::time::Duration;

const ORIGEM: &str = "RAPL do kernel";

#[derive(Debug)]
pub struct CpuRapl {
    pacotes: Vec<Contador>,
    nucleos: Vec<Contador>,
}

impl CpuRapl {
    /// `None` se não houver pacote RAPL legível (sem hardware, VM ou sem permissão).
    #[must_use]
    pub fn detectar(raiz: &RaizDoSistema) -> Option<Self> {
        let base = raiz.caminho("sys/class/powercap");
        let mut zonas: Vec<_> = std::fs::read_dir(base)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .collect();
        zonas.sort();
        let (mut pacotes, mut nucleos) = (Vec::new(), Vec::new());
        for z in zonas {
            let Some(dir) = z.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
                continue;
            };
            if !dir.starts_with("intel-rapl:") {
                continue; // ignora intel-rapl-mmio (duplica o pacote)
            }
            let nome = ler_txt(&z.join("name")).unwrap_or_default();
            let contador =
                Contador::novo(z.join("energy_uj"), ler_u64(&z.join("max_energy_range_uj")));
            match dir.matches(':').count() {
                1 if nome.starts_with("package") => pacotes.push(contador),
                2 if nome == "core" => nucleos.push(contador),
                _ => {}
            }
        }
        if pacotes.is_empty() || !pacotes.iter().all(Contador::legivel) {
            return None;
        }
        if nucleos.len() != pacotes.len() || !nucleos.iter().all(Contador::legivel) {
            nucleos.clear(); // núcleos só valem se existirem em todos os pacotes
        }
        Some(Self { pacotes, nucleos })
    }
}

fn soma(contadores: &mut [Contador], dt: Duration) -> Option<f64> {
    contadores.iter_mut().map(|c| c.ler(dt).map(|w| w.0)).sum()
}

fn amostra(w: Option<f64>) -> Amostra {
    w.map_or(Amostra::SemLeitura, |w| Amostra::Potencia(Watts(w)))
}

impl Medidor for CpuRapl {
    fn componentes(&self) -> Vec<Componente> {
        if self.nucleos.is_empty() {
            vec![Componente::novo("cpu", "CPU", ORIGEM, Perfil::Sensor)]
        } else {
            vec![
                Componente::novo("cpu_nucleos", "CPU núcleos", ORIGEM, Perfil::Sensor),
                Componente::novo("cpu_soc", "CPU resto do chip", ORIGEM, Perfil::Sensor),
            ]
        }
    }

    fn ler(&mut self, dt: Duration) -> Vec<(ComponenteId, Amostra)> {
        let pacote = soma(&mut self.pacotes, dt);
        if self.nucleos.is_empty() {
            return vec![(ComponenteId::new("cpu"), amostra(pacote))];
        }
        let nucleos = soma(&mut self.nucleos, dt);
        let resto = pacote.zip(nucleos).map(|(p, n)| (p - n).max(0.0));
        let nucleos = pacote.and(nucleos);
        vec![
            (ComponenteId::new("cpu_nucleos"), amostra(nucleos)),
            (ComponenteId::new("cpu_soc"), amostra(resto)),
        ]
    }

    fn reiniciar(&mut self) {
        self.pacotes
            .iter_mut()
            .chain(self.nucleos.iter_mut())
            .for_each(Contador::reiniciar);
    }
}
