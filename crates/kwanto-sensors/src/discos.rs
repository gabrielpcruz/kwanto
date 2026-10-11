//! Discos: potência estimada pela fração de tempo ocupado (io_ticks) e pelo estado de standby.
//! IDs por WWN/serial, porque os nomes /dev/sdX e /dev/nvmeX podem mudar entre boots.
use crate::raiz::{RaizDoSistema, ler_txt, ler_u64};
use kwanto_core::componente::{Amostra, Componente, ComponenteId, Perfil};
use kwanto_core::sensores::Medidor;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

const CONSULTA_STANDBY: Duration = Duration::from_secs(30);
const LIMITE_HDPARM: Duration = Duration::from_secs(5);

/// Estado de standby por dispositivo, atualizado por uma thread própria (a consulta pode travar).
type CacheStandby = Arc<Mutex<HashMap<PathBuf, bool>>>;

pub trait ConsultaStandby: Send + Sync {
    /// `Some(true)` em standby, `Some(false)` girando, `None` se não deu para saber.
    fn em_standby(&self, dispositivo: &Path) -> Option<bool>;
}

/// ATA CHECK POWER MODE: não acorda o disco. Usa ioctl e recorre ao `hdparm -C` se falhar.
#[derive(Debug, Default)]
pub struct StandbyAta;

impl StandbyAta {
    #[allow(unsafe_code)]
    fn por_ioctl(dispositivo: &Path) -> Option<bool> {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::OpenOptionsExt;
        const HDIO_DRIVE_CMD: libc::c_ulong = 0x031f;
        const CHECK_POWER_MODE: u8 = 0xE5;
        let arquivo = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(dispositivo)
            .ok()?;
        let mut args: [u8; 4] = [CHECK_POWER_MODE, 0, 0, 0];
        // SAFETY: `arquivo` é um descritor válido enquanto vive neste escopo; HDIO_DRIVE_CMD lê e
        // escreve exatamente 4 bytes em `args`, que é um buffer de 4 bytes exclusivo deste escopo.
        let r = unsafe { libc::ioctl(arquivo.as_raw_fd(), HDIO_DRIVE_CMD as _, args.as_mut_ptr()) };
        (r == 0).then(|| args[2] == 0x00)
    }

    fn por_hdparm(dispositivo: &Path) -> Option<bool> {
        let mut cmd = Command::new("hdparm");
        cmd.arg("-C").arg(dispositivo);
        let saida = saida_com_limite(cmd, LIMITE_HDPARM)?;
        let texto = String::from_utf8_lossy(&saida.stdout);
        texto
            .contains("drive state is")
            .then(|| texto.contains("standby") || texto.contains("sleeping"))
    }
}

impl ConsultaStandby for StandbyAta {
    fn em_standby(&self, dispositivo: &Path) -> Option<bool> {
        Self::por_ioctl(dispositivo).or_else(|| Self::por_hdparm(dispositivo))
    }
}

/// Roda o comando e desiste (matando o processo) se passar do limite.
fn saida_com_limite(mut cmd: Command, limite: Duration) -> Option<Output> {
    let mut filho = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let inicio = Instant::now();
    loop {
        if filho.try_wait().ok()?.is_some() {
            return filho.wait_with_output().ok();
        }
        if inicio.elapsed() >= limite {
            let _ = filho.kill();
            let _ = filho.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[derive(Debug)]
struct Disco {
    componente: Componente,
    stat: PathBuf,
    dev: PathBuf,
    hdd: bool,
    ultimo_ticks: Option<u64>,
}

#[derive(Debug)]
pub struct Discos {
    discos: Vec<Disco>,
    standby: CacheStandby,
}

fn io_ticks(stat: &Path) -> Option<u64> {
    ler_txt(stat)?.split_whitespace().nth(9)?.parse().ok()
}

fn id_estavel(bloco: &Path) -> Option<String> {
    ["wwid", "device/wwid", "device/serial", "device/model"]
        .iter()
        .find_map(|f| ler_txt(&bloco.join(f)).filter(|s| !s.is_empty()))
        .map(|s| s.split_whitespace().collect::<Vec<_>>().join("_"))
}

pub(crate) fn aceito(nome: &str, bloco: &Path) -> bool {
    let prefixo = ["sd", "nvme", "vd", "mmcblk", "hd"]
        .iter()
        .any(|p| nome.starts_with(p));
    prefixo
        && !nome.contains("boot")
        && !nome.contains("rpmb")
        && ler_u64(&bloco.join("size")).unwrap_or(0) > 0
}

impl Discos {
    #[must_use]
    pub fn detectar(raiz: &RaizDoSistema, consulta: Arc<dyn ConsultaStandby>) -> Self {
        let mut blocos: Vec<_> = std::fs::read_dir(raiz.caminho("sys/block"))
            .map(|r| r.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        blocos.sort();
        let mut discos: Vec<Disco> = Vec::new();
        for bloco in blocos {
            let nome = bloco
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if !aceito(&nome, &bloco) {
                continue;
            }
            let Some(base_id) = id_estavel(&bloco) else {
                continue;
            };
            // Duplicatas (mesmo modelo sem serial) ganham "+"; o ID exposto é anônimo.
            let mut bruto = base_id;
            while discos
                .iter()
                .any(|d| d.componente.id == ComponenteId::anonimo("disco", &bruto))
            {
                bruto.push('+');
            }
            let id = ComponenteId::anonimo("disco", &bruto);
            let hdd = ler_txt(&bloco.join("queue/rotational")).as_deref() == Some("1");
            let modelo = ler_txt(&bloco.join("device/model")).unwrap_or_else(|| nome.clone());
            let rotulo = format!("{} {modelo}", if hdd { "HDD" } else { "SSD" });
            let perfil = if hdd { Perfil::Hdd } else { Perfil::Ssd };
            let stat = bloco.join("stat");
            discos.push(Disco {
                componente: Componente::novo(id.as_str(), &rotulo, "atividade do disco", perfil),
                ultimo_ticks: io_ticks(&stat),
                stat,
                dev: raiz.caminho(&format!("dev/{nome}")),
                hdd,
            });
        }
        discos.sort_by(|a, b| a.componente.id.cmp(&b.componente.id));
        let standby = CacheStandby::default();
        let hdds: Vec<PathBuf> = discos
            .iter()
            .filter(|d| d.hdd)
            .map(|d| d.dev.clone())
            .collect();
        if !hdds.is_empty() {
            Self::iniciar_consulta(hdds, consulta, Arc::clone(&standby));
        }
        Self { discos, standby }
    }

    /// Thread que consulta o standby dos HDDs a cada 30 s, sem bloquear a coleta.
    fn iniciar_consulta(
        hdds: Vec<PathBuf>,
        consulta: Arc<dyn ConsultaStandby>,
        cache: CacheStandby,
    ) {
        let resultado = std::thread::Builder::new()
            .name("kwanto-standby".into())
            .spawn(move || {
                loop {
                    for dev in &hdds {
                        let parado = consulta.em_standby(dev).unwrap_or(false);
                        cache
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .insert(dev.clone(), parado);
                    }
                    std::thread::sleep(CONSULTA_STANDBY);
                }
            });
        if let Err(e) = resultado {
            tracing::warn!(erro = %e, "não foi possível iniciar a consulta de standby dos discos");
        }
    }

    #[must_use]
    pub fn tem_hdd(&self) -> bool {
        self.discos.iter().any(|d| d.hdd)
    }
}

impl Medidor for Discos {
    fn componentes(&self) -> Vec<Componente> {
        self.discos.iter().map(|d| d.componente.clone()).collect()
    }

    fn ler(&mut self, dt: Duration) -> Vec<(ComponenteId, Amostra)> {
        let standby = self
            .standby
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        self.discos
            .iter_mut()
            .map(|d| {
                let atual = io_ticks(&d.stat);
                let anterior = std::mem::replace(&mut d.ultimo_ticks, atual);
                let parado = d.hdd && standby.get(&d.dev).copied().unwrap_or(false);
                let amostra = match (anterior, atual) {
                    (_, None) => Amostra::SemLeitura,
                    (Some(a), Some(b)) if !dt.is_zero() => {
                        #[allow(clippy::cast_precision_loss)]
                        let ocupado = (b.saturating_sub(a) as f64 / (dt.as_secs_f64() * 1000.0))
                            .clamp(0.0, 1.0);
                        if parado && ocupado == 0.0 {
                            Amostra::Standby
                        } else {
                            Amostra::Atividade(ocupado)
                        }
                    }
                    _ if parado => Amostra::Standby,
                    _ => Amostra::Atividade(0.0),
                };
                (d.componente.id.clone(), amostra)
            })
            .collect()
    }

    fn reiniciar(&mut self) {
        for d in &mut self.discos {
            d.ultimo_ticks = io_ticks(&d.stat);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn comando_que_passa_do_limite_e_encerrado() {
        let mut cmd = Command::new("sleep");
        cmd.arg("10");
        let inicio = Instant::now();
        assert!(saida_com_limite(cmd, Duration::from_millis(200)).is_none());
        assert!(inicio.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn comando_rapido_devolve_a_saida() {
        let mut cmd = Command::new("echo");
        cmd.arg("drive state is:  standby");
        let saida = saida_com_limite(cmd, Duration::from_secs(5)).expect("saída");
        assert!(String::from_utf8_lossy(&saida.stdout).contains("standby"));
    }
}
