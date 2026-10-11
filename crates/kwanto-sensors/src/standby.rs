//! Standby dos HDDs separado do serviço principal. Consultar o disco exige o grupo `disk` e
//! `CAP_SYS_RAWIO`, que dariam acesso bruto aos discos a quem tomasse o serviço que fala HTTP. Por
//! isso um processo mínimo (`kwanto-standby`, sem rede) consulta e grava um arquivo, e o `kwantod`,
//! sem nenhum privilégio, só o lê.
use crate::discos::{ConsultaStandby, aceito};
use crate::raiz::{RaizDoSistema, ler_txt};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Onde o `kwanto-standby` grava (diretório de execução do serviço).
pub const CAMINHO_ESTADO: &str = "/run/kwanto-standby/estado.json";
/// Estado mais velho que isso (o consultor parou) não é usado.
const VALIDADE: Duration = Duration::from_secs(90);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EstadoStandby {
    /// Segundos Unix da consulta.
    pub atualizado: i64,
    /// Nome do dispositivo (`sda`) → em standby. Disco que não respondeu fica de fora.
    pub discos: BTreeMap<String, bool>,
}

/// `/dev/sdX` dos discos rotacionais (só eles têm standby que importa).
#[must_use]
pub fn hdds(raiz: &RaizDoSistema) -> Vec<PathBuf> {
    let mut blocos: Vec<_> = std::fs::read_dir(raiz.caminho("sys/block"))
        .map(|r| r.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    blocos.sort();
    blocos
        .iter()
        .filter_map(|b| {
            let nome = b.file_name()?.to_str()?;
            (nome.starts_with("sd")
                && aceito(nome, b)
                && ler_txt(&b.join("queue/rotational")).as_deref() == Some("1"))
            .then(|| raiz.caminho(&format!("dev/{nome}")))
        })
        .collect()
}

#[must_use]
pub fn consultar(
    raiz: &RaizDoSistema,
    consulta: &dyn ConsultaStandby,
    agora: i64,
) -> EstadoStandby {
    let discos = hdds(raiz)
        .iter()
        .filter_map(|dev| {
            let nome = dev.file_name()?.to_str()?.to_string();
            consulta.em_standby(dev).map(|parado| (nome, parado))
        })
        .collect();
    EstadoStandby {
        atualizado: agora,
        discos,
    }
}

/// Grava de forma atômica (arquivo temporário + rename), legível só pelo grupo kwanto (0640).
pub fn gravar(estado: &EstadoStandby, destino: &Path) -> std::io::Result<()> {
    let temporario = destino.with_extension("json.novo");
    let _ = std::fs::remove_file(&temporario);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o640)
        .open(&temporario)?;
    f.write_all(&serde_json::to_vec(estado).map_err(std::io::Error::other)?)?;
    f.sync_all()?;
    std::fs::rename(&temporario, destino)
}

fn agora() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0)
}

/// Lado do `kwantod`: lê o arquivo gravado pelo `kwanto-standby`.
#[derive(Debug)]
pub struct StandbyArquivo {
    caminho: PathBuf,
}

impl StandbyArquivo {
    #[must_use]
    pub fn novo(caminho: &Path) -> Self {
        Self {
            caminho: caminho.to_path_buf(),
        }
    }
}

impl ConsultaStandby for StandbyArquivo {
    fn em_standby(&self, dispositivo: &Path) -> Option<bool> {
        let bytes = std::fs::read(&self.caminho).ok()?;
        let estado: EstadoStandby = serde_json::from_slice(&bytes).ok()?;
        let idade = agora().saturating_sub(estado.atualizado);
        if !(0..=i64::try_from(VALIDADE.as_secs()).unwrap_or(i64::MAX)).contains(&idade) {
            return None;
        }
        estado
            .discos
            .get(dispositivo.file_name()?.to_str()?)
            .copied()
    }
}
