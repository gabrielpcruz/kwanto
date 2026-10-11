//! GPUs dedicadas AMD e Intel com sensor no hwmon do kernel. NVIDIA fica com o NVML.
use crate::contador::Contador;
use crate::raiz::{RaizDoSistema, ler_txt, ler_u64};
use kwanto_core::componente::{Amostra, Componente, ComponenteId, Perfil};
use kwanto_core::sensores::Medidor;
use kwanto_core::unidades::Watts;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Placa em economia de energia (runtime suspend): não consultar, para não acordá-la.
#[must_use]
pub fn gpu_suspensa(dispositivo: &Path) -> bool {
    ler_txt(&dispositivo.join("power/runtime_status")).as_deref() == Some("suspended")
}

#[derive(Debug)]
enum Fonte {
    Potencia(PathBuf),
    Energia(Contador),
}

#[derive(Debug)]
struct Gpu {
    componente: Componente,
    dispositivo: PathBuf,
    fonte: Fonte,
}

#[derive(Debug)]
pub struct GpusHwmon {
    gpus: Vec<Gpu>,
}

const DOIS_GIB: u64 = 2 * 1024 * 1024 * 1024;

/// GPUs integradas já entram no RAPL da CPU; contá-las de novo duplicaria.
fn integrada(vendor: &str, pci: &str, dispositivo: &Path) -> bool {
    match vendor {
        "0x8086" => pci.ends_with("00:02.0"),
        "0x1002" => ler_u64(&dispositivo.join("mem_info_vram_total")).unwrap_or(0) < DOIS_GIB,
        _ => false,
    }
}

fn fonte_do_hwmon(dispositivo: &Path) -> Option<Fonte> {
    let mut hwmons: Vec<_> = std::fs::read_dir(dispositivo.join("hwmon"))
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    hwmons.sort();
    hwmons.into_iter().find_map(|hw| {
        let potencia = ["power1_average", "power1_input"]
            .iter()
            .map(|f| hw.join(f))
            .find(|p| p.exists());
        match potencia {
            Some(p) => Some(Fonte::Potencia(p)),
            None => {
                let energia = hw.join("energy1_input");
                energia
                    .exists()
                    .then(|| Fonte::Energia(Contador::novo(energia, None)))
            }
        }
    })
}

impl GpusHwmon {
    #[must_use]
    pub fn detectar(raiz: &RaizDoSistema) -> Option<Self> {
        let mut cards: Vec<_> = std::fs::read_dir(raiz.caminho("sys/class/drm"))
            .ok()?
            .flatten()
            .map(|e| e.path())
            .collect();
        cards.sort();
        let mut vistos = Vec::new();
        let mut gpus = Vec::new();
        for card in cards {
            let nome = card
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if !nome.starts_with("card") || nome.contains('-') {
                continue; // conectores como card0-HDMI-A-1
            }
            let Ok(dispositivo) = card.join("device").canonicalize() else {
                continue;
            };
            if vistos.contains(&dispositivo) {
                continue;
            }
            vistos.push(dispositivo.clone());
            let vendor = ler_txt(&dispositivo.join("vendor")).unwrap_or_default();
            let pci = dispositivo
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if vendor == "0x10de" || integrada(&vendor, &pci, &dispositivo) {
                continue;
            }
            let Some(fonte) = fonte_do_hwmon(&dispositivo) else {
                continue;
            };
            let fabricante = match vendor.as_str() {
                "0x1002" => "AMD",
                "0x8086" => "Intel",
                _ => "dedicada",
            };
            let componente = Componente::novo(
                &format!("gpu:{pci}"),
                &format!("GPU {fabricante} ({pci})"),
                "sensor do kernel",
                Perfil::Sensor,
            );
            gpus.push(Gpu {
                componente,
                dispositivo,
                fonte,
            });
        }
        (!gpus.is_empty()).then_some(Self { gpus })
    }
}

impl Medidor for GpusHwmon {
    fn componentes(&self) -> Vec<Componente> {
        self.gpus.iter().map(|g| g.componente.clone()).collect()
    }

    fn ler(&mut self, dt: Duration) -> Vec<(ComponenteId, Amostra)> {
        self.gpus
            .iter_mut()
            .map(|g| {
                let amostra = if gpu_suspensa(&g.dispositivo) {
                    Amostra::Potencia(Watts(0.0))
                } else {
                    let w = match &mut g.fonte {
                        #[allow(clippy::cast_precision_loss)]
                        Fonte::Potencia(p) => ler_u64(p).map(|uw| Watts(uw as f64 / 1e6)),
                        Fonte::Energia(c) => c.ler(dt),
                    };
                    w.map_or(Amostra::SemLeitura, Amostra::Potencia)
                };
                (g.componente.id.clone(), amostra)
            })
            .collect()
    }

    fn reiniciar(&mut self) {
        for g in &mut self.gpus {
            if let Fonte::Energia(c) = &mut g.fonte {
                c.reiniciar();
            }
        }
    }
}
