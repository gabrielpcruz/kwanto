//! Monta a lista de medidores disponíveis nesta máquina Linux.
use crate::cpu_uso::CpuPorUso;
use crate::discos::Discos;
use crate::fixos::ValoresFixos;
use crate::hwmon::GpusHwmon;
use crate::nvidia::{FonteNvidia, GpusNvidia, NvmlReal};
use crate::raiz::{RaizDoSistema, ler_txt};
use crate::rapl::CpuRapl;
use crate::standby::{CAMINHO_ESTADO, StandbyArquivo};
use kwanto_core::sensores::{DetectorDeHardware, Hardware, Medidor};
use std::sync::Arc;
use std::time::Duration;

/// Tipos de chassi SMBIOS de notebook/portátil.
const CHASSI_NOTEBOOK: [&str; 8] = ["8", "9", "10", "11", "14", "30", "31", "32"];
/// O serviço pode subir antes do driver NVIDIA no boot: tenta por até 30 s.
const TENTATIVAS_NVML: u32 = 15;
const ESPERA_NVML: Duration = Duration::from_secs(2);

#[must_use]
pub fn eh_notebook(raiz: &RaizDoSistema) -> bool {
    ler_txt(&raiz.caminho("sys/class/dmi/id/chassis_type"))
        .is_some_and(|t| CHASSI_NOTEBOOK.contains(&t.as_str()))
}

/// Há placa NVIDIA no barramento PCI? Sem isso, nem tenta o NVML.
fn nvidia_presente(raiz: &RaizDoSistema) -> bool {
    std::fs::read_dir(raiz.caminho("sys/bus/pci/devices"))
        .map(|r| {
            r.flatten()
                .any(|e| ler_txt(&e.path().join("vendor")).as_deref() == Some("0x10de"))
        })
        .unwrap_or(false)
}

/// Inicia o NVML tentando de novo enquanto o driver carrega; avisa no log se desistir.
pub fn iniciar_nvidia(
    raiz: &RaizDoSistema,
    mut iniciar: impl FnMut() -> Result<Box<dyn FonteNvidia>, String>,
    tentativas: u32,
    espera: Duration,
) -> Option<GpusNvidia> {
    let mut ultimo_erro = String::new();
    for i in 0..tentativas {
        match iniciar() {
            Ok(fonte) => return GpusNvidia::com_fonte(fonte, raiz.clone()),
            Err(e) => ultimo_erro = e,
        }
        if i + 1 < tentativas {
            std::thread::sleep(espera);
        }
    }
    tracing::warn!(
        erro = %ultimo_erro,
        "há GPU NVIDIA, mas o driver não respondeu; ela não aparecerá até o serviço reiniciar"
    );
    None
}

#[derive(Debug)]
pub struct DetectorLinux {
    raiz: RaizDoSistema,
    usar_nvidia: bool,
}

impl DetectorLinux {
    #[must_use]
    pub fn novo(raiz: RaizDoSistema) -> Self {
        Self {
            raiz,
            usar_nvidia: true,
        }
    }

    /// Para testes: não tenta carregar o NVML real.
    #[must_use]
    pub fn sem_nvidia(raiz: RaizDoSistema) -> Self {
        Self {
            raiz,
            usar_nvidia: false,
        }
    }
}

impl DetectorDeHardware for DetectorLinux {
    fn detectar(&self) -> Hardware {
        let mut medidores: Vec<Box<dyn Medidor>> = Vec::new();
        let rapl = CpuRapl::detectar(&self.raiz);
        let cpu_estimada = rapl.is_none();
        match rapl {
            Some(r) => medidores.push(Box::new(r)),
            None => medidores.push(Box::new(CpuPorUso::novo(&self.raiz))),
        }
        if self.usar_nvidia && nvidia_presente(&self.raiz) {
            let iniciar = || NvmlReal::iniciar().map(|n| Box::new(n) as Box<dyn FonteNvidia>);
            if let Some(g) = iniciar_nvidia(&self.raiz, iniciar, TENTATIVAS_NVML, ESPERA_NVML) {
                medidores.push(Box::new(g));
            }
        }
        if let Some(g) = GpusHwmon::detectar(&self.raiz) {
            medidores.push(Box::new(g));
        }
        medidores.push(Box::new(ValoresFixos));
        // O standby vem do serviço kwanto-standby: este processo não tem acesso aos discos.
        let discos = Discos::detectar(
            &self.raiz,
            Arc::new(StandbyArquivo::novo(std::path::Path::new(CAMINHO_ESTADO))),
        );
        let tem_hdd = discos.tem_hdd();
        if !discos.componentes().is_empty() {
            medidores.push(Box::new(discos));
        }
        Hardware {
            medidores,
            cpu_estimada,
            tem_hdd,
            notebook: eh_notebook(&self.raiz),
        }
    }
}
