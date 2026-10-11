//! GPUs NVIDIA pelo NVML. As chamadas rodam numa thread própria, porque podem travar.
use crate::hwmon::gpu_suspensa;
use crate::raiz::RaizDoSistema;
use kwanto_core::componente::{Amostra, Componente, ComponenteId, Perfil};
use kwanto_core::sensores::Medidor;
use kwanto_core::unidades::Watts;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

const LEITURA_VELHA: Duration = Duration::from_secs(10);
const INTERVALO_COLETA: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InfoGpu {
    pub uuid: String,
    pub nome: String,
    /// Endereço PCI no formato do sysfs, ex.: `0000:0a:00.0`.
    pub pci: String,
}

pub trait FonteNvidia: Send {
    fn listar(&mut self) -> Result<Vec<InfoGpu>, String>;
    fn potencia(&mut self, uuid: &str) -> Result<Watts, String>;
    /// A placa está trabalhando? Na dúvida, sim (assim o driver não é solto à toa).
    fn ocupada(&mut self, _uuid: &str) -> Result<bool, String> {
        Ok(true)
    }
    /// Solta o acesso ao driver para a placa poder entrar em economia de energia (RTD3).
    fn liberar(&mut self) {}
}

pub type Cache = Arc<Mutex<HashMap<String, (Watts, Instant)>>>;

/// Converte o barramento do NVML ("00000000:0A:00.0") para o formato do sysfs ("0000:0a:00.0").
#[must_use]
pub fn pci_do_nvml(bus_id: &str) -> String {
    let n = bus_id.len();
    bus_id[n.saturating_sub(12)..].to_lowercase()
}

/// NVML real; tenta reinicializar depois de erros (ex.: driver atualizado sem reiniciar).
pub struct NvmlReal {
    nvml: Option<nvml_wrapper::Nvml>,
}

impl std::fmt::Debug for NvmlReal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NvmlReal")
    }
}

impl NvmlReal {
    /// Erro se o driver NVIDIA não responder (não instalado ou ainda carregando).
    pub fn iniciar() -> Result<Self, String> {
        nvml_wrapper::Nvml::init()
            .map(|n| Self { nvml: Some(n) })
            .map_err(|e| e.to_string())
    }

    fn nvml(&mut self) -> Result<&nvml_wrapper::Nvml, String> {
        if self.nvml.is_none() {
            self.nvml = Some(nvml_wrapper::Nvml::init().map_err(|e| e.to_string())?);
        }
        self.nvml
            .as_ref()
            .ok_or_else(|| "NVML indisponível".to_string())
    }
}

impl FonteNvidia for NvmlReal {
    fn listar(&mut self) -> Result<Vec<InfoGpu>, String> {
        let nvml = self.nvml()?;
        let n = nvml.device_count().map_err(|e| e.to_string())?;
        (0..n)
            .map(|i| {
                let d = nvml.device_by_index(i).map_err(|e| e.to_string())?;
                Ok(InfoGpu {
                    uuid: d.uuid().map_err(|e| e.to_string())?,
                    nome: d.name().map_err(|e| e.to_string())?,
                    pci: pci_do_nvml(&d.pci_info().map_err(|e| e.to_string())?.bus_id),
                })
            })
            .collect()
    }

    fn potencia(&mut self, uuid: &str) -> Result<Watts, String> {
        let resultado = self
            .nvml()
            .and_then(|n| n.device_by_uuid(uuid).map_err(|e| e.to_string()))
            .and_then(|d| d.power_usage().map_err(|e| e.to_string()));
        match resultado {
            Ok(mw) => Ok(Watts(f64::from(mw) / 1000.0)),
            Err(e) => {
                self.nvml = None; // força reinicialização na próxima rodada
                Err(e)
            }
        }
    }

    fn ocupada(&mut self, uuid: &str) -> Result<bool, String> {
        let u = self
            .nvml()?
            .device_by_uuid(uuid)
            .and_then(|d| d.utilization_rates())
            .map_err(|e| e.to_string())?;
        Ok(u.gpu > 0 || u.memory > 0)
    }

    fn liberar(&mut self) {
        self.nvml = None; // fecha /dev/nvidia*; reabre na próxima leitura
    }
}

/// Placa de vídeo principal (a da tela no boot): nunca dorme enquanto houver tela.
fn gpu_principal(dispositivo: &std::path::Path) -> bool {
    crate::raiz::ler_txt(&dispositivo.join("boot_vga")).as_deref() == Some("1")
}

/// Leituras seguidas sem uso antes de soltar o driver.
const OCIOSAS_PARA_LIBERAR: u32 = 3;
/// Depois de soltar o driver, quanto esperar a placa dormir sem consultá-la.
const PRAZO_PARA_DORMIR: Duration = Duration::from_secs(30);
/// Se ela não dormiu (outro programa a usa), espera antes de tentar de novo; dobra a cada falha.
const ESPERA_INICIAL: Duration = Duration::from_secs(60);
const ESPERA_MAXIMA: Duration = Duration::from_secs(3600);

/// Placa com economia automática de energia: segurar o driver aberto a impediria de dormir.
fn economia_automatica(dispositivo: &std::path::Path) -> bool {
    crate::raiz::ler_txt(&dispositivo.join("power/control")).as_deref() == Some("auto")
}

/// Faz uma rodada de leituras e guarda no cache. Separado da thread para ser testável.
pub struct ColetorNvidia {
    fonte: Box<dyn FonteNvidia>,
    raiz: RaizDoSistema,
    gpus: Vec<InfoGpu>,
    cache: Cache,
    /// Soltar e reabrir o driver da NVIDIA vaza um descritor a cada ciclo; por isso só se solta
    /// quando adianta (placa dedicada, ociosa) e com esperas crescentes se ela não dormir.
    ociosas: u32,
    aguardando_dormir: Option<Instant>,
    espera: Duration,
    proxima_liberacao: Option<Instant>,
    ultimas: HashMap<String, Watts>,
}

impl std::fmt::Debug for ColetorNvidia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ColetorNvidia")
            .field("gpus", &self.gpus)
            .finish_non_exhaustive()
    }
}

impl ColetorNvidia {
    #[must_use]
    pub fn novo(fonte: Box<dyn FonteNvidia>, raiz: RaizDoSistema, gpus: Vec<InfoGpu>) -> Self {
        Self {
            fonte,
            raiz,
            gpus,
            cache: Arc::default(),
            ociosas: 0,
            aguardando_dormir: None,
            espera: ESPERA_INICIAL,
            proxima_liberacao: None,
            ultimas: HashMap::new(),
        }
    }

    #[must_use]
    pub fn cache(&self) -> Cache {
        Arc::clone(&self.cache)
    }

    pub fn rodada(&mut self) {
        self.rodada_em(Instant::now());
    }

    fn guardar(&self, uuid: &str, w: Watts, agora: Instant) {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(uuid.to_string(), (w, agora));
    }

    /// Uma rodada de leituras no instante dado (separado para os testes controlarem o tempo).
    pub fn rodada_em(&mut self, agora: Instant) {
        let dispositivos: Vec<_> = self
            .gpus
            .iter()
            .map(|g| self.raiz.caminho(&format!("sys/bus/pci/devices/{}", g.pci)))
            .collect();
        let pode_liberar = !dispositivos.is_empty()
            && dispositivos
                .iter()
                .all(|d| economia_automatica(d) && !gpu_principal(d));
        let suspensas: Vec<bool> = dispositivos.iter().map(|d| gpu_suspensa(d)).collect();
        let todas_dormindo = suspensas.iter().all(|s| *s);

        if let Some(desde) = self.aguardando_dormir {
            if todas_dormindo {
                self.aguardando_dormir = None;
                self.espera = ESPERA_INICIAL;
            } else if agora.saturating_duration_since(desde) < PRAZO_PARA_DORMIR {
                // Não reabre o driver enquanto dá tempo de a placa dormir: repete a última leitura.
                for g in &self.gpus {
                    let w = self.ultimas.get(&g.uuid).copied().unwrap_or(Watts(0.0));
                    self.guardar(&g.uuid, w, agora);
                }
                return;
            } else {
                // Não dormiu: outro programa a usa. Segura o driver e espera mais na próxima vez.
                self.aguardando_dormir = None;
                self.proxima_liberacao = Some(agora + self.espera);
                self.espera = (self.espera * 2).min(ESPERA_MAXIMA);
            }
        }

        let mut todas_ociosas = true;
        for (i, g) in self.gpus.clone().iter().enumerate() {
            let leitura = if suspensas[i] {
                Ok(Watts(0.0)) // não acorda a placa em notebook híbrido
            } else {
                self.fonte.potencia(&g.uuid)
            };
            match leitura {
                Ok(w) => {
                    self.ultimas.insert(g.uuid.clone(), w);
                    self.guardar(&g.uuid, w, agora);
                }
                Err(e) => {
                    tracing::debug!(gpu = %g.uuid, erro = %e, "falha ao ler a GPU NVIDIA");
                    self.cache
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .remove(&g.uuid);
                }
            }
            if pode_liberar && !suspensas[i] {
                todas_ociosas &= !self.fonte.ocupada(&g.uuid).unwrap_or(true);
            }
        }
        if !pode_liberar || todas_dormindo {
            self.ociosas = 0;
            return;
        }
        self.ociosas = if todas_ociosas { self.ociosas + 1 } else { 0 };
        let na_hora = self.proxima_liberacao.is_none_or(|p| agora >= p);
        if self.ociosas >= OCIOSAS_PARA_LIBERAR && na_hora {
            self.fonte.liberar();
            self.aguardando_dormir = Some(agora);
            self.ociosas = 0;
        }
    }
}

/// Leitura do cache: velha ou ausente vira `SemLeitura`.
#[must_use]
pub fn ler_cache(cache: &Cache, uuid: &str, agora: Instant) -> Amostra {
    let c = cache.lock().unwrap_or_else(PoisonError::into_inner);
    match c.get(uuid) {
        Some((w, quando)) if agora.saturating_duration_since(*quando) < LEITURA_VELHA => {
            Amostra::Potencia(*w)
        }
        _ => Amostra::SemLeitura,
    }
}

#[derive(Debug)]
pub struct GpusNvidia {
    componentes: Vec<(String, Componente)>,
    cache: Cache,
}

impl GpusNvidia {
    /// Lista as GPUs e inicia a thread de coleta. `None` se não houver GPU.
    pub fn com_fonte(mut fonte: Box<dyn FonteNvidia>, raiz: RaizDoSistema) -> Option<Self> {
        let gpus = fonte.listar().ok().filter(|g| !g.is_empty())?;
        let componentes = gpus
            .iter()
            .map(|g| {
                let c = Componente::novo(
                    ComponenteId::anonimo("gpu", &g.uuid).as_str(),
                    &g.nome,
                    "NVML (driver NVIDIA)",
                    Perfil::Sensor,
                );
                (g.uuid.clone(), c)
            })
            .collect();
        let mut coletor = ColetorNvidia::novo(fonte, raiz, gpus);
        let cache = coletor.cache();
        std::thread::Builder::new()
            .name("kwanto-nvml".into())
            .spawn(move || {
                loop {
                    coletor.rodada();
                    std::thread::sleep(INTERVALO_COLETA);
                }
            })
            .ok()?;
        Some(Self { componentes, cache })
    }
}

impl Medidor for GpusNvidia {
    fn componentes(&self) -> Vec<Componente> {
        self.componentes.iter().map(|(_, c)| c.clone()).collect()
    }

    fn ler(&mut self, _dt: Duration) -> Vec<(ComponenteId, Amostra)> {
        let agora = Instant::now();
        self.componentes
            .iter()
            .map(|(uuid, c)| (c.id.clone(), ler_cache(&self.cache, uuid, agora)))
            .collect()
    }
}
