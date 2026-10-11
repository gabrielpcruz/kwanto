use kwanto_core::componente::Amostra;
use kwanto_core::sensores::{DetectorDeHardware, Medidor};
use kwanto_sensors::detector::{DetectorLinux, eh_notebook};
use kwanto_sensors::discos::{ConsultaStandby, Discos};
use kwanto_sensors::raiz::RaizDoSistema;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn escrever(base: &Path, rel: &str, conteudo: &str) {
    let p = base.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, conteudo).unwrap();
}

/// stat: 11+ campos; o 10º (índice 9) é io_ticks em ms.
fn stat(io_ticks: u64) -> String {
    format!("1 0 0 0 1 0 0 0 0 {io_ticks} 0\n")
}

fn disco(base: &Path, dev: &str, modelo: &str, rot: &str, extra: &[(&str, &str)]) {
    escrever(base, &format!("sys/block/{dev}/size"), "1000");
    escrever(base, &format!("sys/block/{dev}/device/model"), modelo);
    escrever(base, &format!("sys/block/{dev}/queue/rotational"), rot);
    escrever(base, &format!("sys/block/{dev}/stat"), &stat(0));
    for (rel, v) in extra {
        escrever(base, &format!("sys/block/{dev}/{rel}"), v);
    }
}

struct Standby(bool);
impl ConsultaStandby for Standby {
    fn em_standby(&self, _d: &Path) -> Option<bool> {
        Some(self.0)
    }
}

#[test]
fn ids_estaveis_por_wwid_serial_e_modelo_com_duplicata() {
    let t = tempfile::tempdir().unwrap();
    disco(
        t.path(),
        "sda",
        "EXEMPLO4000",
        "1",
        &[("device/wwid", "naa.0000000000000001")],
    );
    disco(
        t.path(),
        "nvme0n1",
        "SSD EXEMPLO 512",
        "0",
        &[("device/serial", "1000000001")],
    );
    disco(t.path(), "sdb", "Generic", "0", &[]);
    disco(t.path(), "sdc", "Generic", "0", &[]);
    disco(t.path(), "mmcblk0boot0", "x", "0", &[]);
    disco(t.path(), "sdd", "Leitor", "0", &[]);
    escrever(t.path(), "sys/block/sdd/size", "0");
    escrever(t.path(), "sys/block/loop0/size", "10");
    let d = Discos::detectar(
        &RaizDoSistema::em(t.path().to_path_buf()),
        Arc::new(Standby(false)),
    );
    let ids: Vec<String> = d
        .componentes()
        .iter()
        .map(|c| c.id.as_str().to_string())
        .collect();
    // IDs estáveis, mas sem o WWID/serial: a API não revela o identificador do hardware.
    let anon = |bruto: &str| {
        kwanto_core::componente::ComponenteId::anonimo("disco", bruto)
            .as_str()
            .to_string()
    };
    let mut esperados = vec![
        anon("1000000001"),
        anon("Generic"),
        anon("Generic+"),
        anon("naa.0000000000000001"),
    ];
    esperados.sort();
    assert_eq!(ids, esperados);
    for id in &ids {
        assert!(
            !id.contains("1000000001") && !id.contains("naa.") && !id.contains("Generic"),
            "{id}"
        );
    }
    assert!(d.tem_hdd());
    let hdd = d
        .componentes()
        .into_iter()
        .find(|c| {
            c.id == kwanto_core::componente::ComponenteId::anonimo("disco", "naa.0000000000000001")
        })
        .unwrap();
    assert_eq!(hdd.rotulo, "HDD EXEMPLO4000");
}

#[test]
fn atividade_standby_e_disco_removido() {
    let t = tempfile::tempdir().unwrap();
    disco(t.path(), "sda", "HD", "1", &[("device/wwid", "w1")]);
    let raiz = RaizDoSistema::em(t.path().to_path_buf());
    let mut d = Discos::detectar(&raiz, Arc::new(Standby(false)));
    escrever(t.path(), "sys/block/sda/stat", &stat(500)); // 500 ms ocupado em 2 s
    assert_eq!(d.ler(Duration::from_secs(2))[0].1, Amostra::Atividade(0.25));

    // a consulta de standby roda numa thread própria: espera ela preencher o estado
    let mut d = Discos::detectar(&raiz, Arc::new(Standby(true)));
    let limite = std::time::Instant::now() + Duration::from_secs(2);
    while d.ler(Duration::from_secs(2))[0].1 != Amostra::Standby {
        assert!(
            std::time::Instant::now() < limite,
            "o standby nunca apareceu"
        );
        std::thread::sleep(Duration::from_millis(20));
    }

    fs::remove_file(t.path().join("sys/block/sda/stat")).unwrap();
    assert_eq!(d.ler(Duration::from_secs(2))[0].1, Amostra::SemLeitura);
}

#[test]
fn detector_sem_rapl_usa_cpu_estimada_e_reconhece_notebook() {
    let t = tempfile::tempdir().unwrap();
    escrever(t.path(), "proc/stat", "cpu  1 0 1 8 0 0 0 0\n");
    escrever(t.path(), "sys/class/dmi/id/chassis_type", "10\n");
    let raiz = RaizDoSistema::em(t.path().to_path_buf());
    assert!(eh_notebook(&raiz));
    let hw = DetectorLinux::sem_nvidia(raiz).detectar();
    assert!(hw.cpu_estimada);
    assert!(hw.notebook);
    let ids: Vec<String> = hw
        .medidores
        .iter()
        .flat_map(|m| m.componentes())
        .map(|c| c.id.as_str().to_string())
        .collect();
    assert_eq!(ids, vec!["cpu", "ram", "placa_mae", "fans_usb"]);
}

struct StandbyLento;
impl ConsultaStandby for StandbyLento {
    fn em_standby(&self, _d: &Path) -> Option<bool> {
        std::thread::sleep(Duration::from_secs(3)); // disco USB travado
        Some(false)
    }
}

#[test]
fn disco_travado_nao_trava_a_leitura() {
    let t = tempfile::tempdir().unwrap();
    disco(t.path(), "sda", "HD", "1", &[("device/wwid", "w1")]);
    let mut d = Discos::detectar(
        &RaizDoSistema::em(t.path().to_path_buf()),
        Arc::new(StandbyLento),
    );
    let inicio = std::time::Instant::now();
    d.ler(Duration::from_secs(2));
    assert!(
        inicio.elapsed() < Duration::from_millis(500),
        "a leitura esperou a consulta de standby"
    );
}
