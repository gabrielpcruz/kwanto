use kwanto_core::componente::{Amostra, ComponenteId};
use kwanto_core::sensores::Medidor;
use kwanto_core::unidades::Watts;
use kwanto_sensors::cpu_uso::CpuPorUso;
use kwanto_sensors::raiz::RaizDoSistema;
use kwanto_sensors::rapl::CpuRapl;
use std::fs;
use std::path::Path;
use std::time::Duration;

fn escrever(base: &Path, rel: &str, conteudo: &str) {
    let p = base.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, conteudo).unwrap();
}

fn zona(base: &Path, nome_dir: &str, nome: &str, uj: u64, max: u64) {
    let d = format!("sys/class/powercap/{nome_dir}");
    escrever(base, &format!("{d}/name"), &format!("{nome}\n"));
    escrever(base, &format!("{d}/energy_uj"), &format!("{uj}\n"));
    escrever(
        base,
        &format!("{d}/max_energy_range_uj"),
        &format!("{max}\n"),
    );
}

fn mapa(v: Vec<(ComponenteId, Amostra)>) -> std::collections::BTreeMap<String, Amostra> {
    v.into_iter()
        .map(|(id, a)| (id.as_str().to_string(), a))
        .collect()
}

#[test]
fn rapl_dois_sockets_sem_core_soma_e_trata_virada() {
    let t = tempfile::tempdir().unwrap();
    zona(t.path(), "intel-rapl:0", "package-0", 1_000_000, 5_000_000);
    zona(t.path(), "intel-rapl:1", "package-1", 4_500_000, 5_000_000);
    zona(t.path(), "intel-rapl-mmio:0", "package-0", 0, 5_000_000); // deve ser ignorada
    let raiz = RaizDoSistema::em(t.path().to_path_buf());
    let mut m = CpuRapl::detectar(&raiz).expect("rapl");
    assert_eq!(m.componentes().len(), 1);
    escrever(
        t.path(),
        "sys/class/powercap/intel-rapl:0/energy_uj",
        "3000000",
    ); // +2 J
    escrever(
        t.path(),
        "sys/class/powercap/intel-rapl:1/energy_uj",
        "500000",
    ); // virou: +1 J
    let l = mapa(m.ler(Duration::from_secs(1)));
    assert_eq!(l["cpu"], Amostra::Potencia(Watts(3.0)));
}

#[test]
fn rapl_com_core_separa_nucleos_do_resto() {
    let t = tempfile::tempdir().unwrap();
    zona(t.path(), "intel-rapl:0", "package-0", 0, 100_000_000);
    zona(t.path(), "intel-rapl:0:0", "core", 0, 100_000_000);
    let raiz = RaizDoSistema::em(t.path().to_path_buf());
    let mut m = CpuRapl::detectar(&raiz).unwrap();
    escrever(
        t.path(),
        "sys/class/powercap/intel-rapl:0/energy_uj",
        "6000000",
    );
    escrever(
        t.path(),
        "sys/class/powercap/intel-rapl:0:0/energy_uj",
        "2000000",
    );
    let l = mapa(m.ler(Duration::from_secs(2)));
    assert_eq!(l["cpu_nucleos"], Amostra::Potencia(Watts(1.0)));
    assert_eq!(l["cpu_soc"], Amostra::Potencia(Watts(2.0)));
}

#[test]
fn rapl_ausente_ou_ilegivel_nao_e_detectado() {
    let t = tempfile::tempdir().unwrap();
    assert!(CpuRapl::detectar(&RaizDoSistema::em(t.path().to_path_buf())).is_none());
    escrever(
        t.path(),
        "sys/class/powercap/intel-rapl:0/name",
        "package-0",
    );
    assert!(CpuRapl::detectar(&RaizDoSistema::em(t.path().to_path_buf())).is_none());
}

#[test]
fn rapl_que_some_durante_a_execucao_vira_sem_leitura() {
    let t = tempfile::tempdir().unwrap();
    zona(t.path(), "intel-rapl:0", "package-0", 0, 100);
    let mut m = CpuRapl::detectar(&RaizDoSistema::em(t.path().to_path_buf())).unwrap();
    fs::remove_file(t.path().join("sys/class/powercap/intel-rapl:0/energy_uj")).unwrap();
    assert_eq!(
        mapa(m.ler(Duration::from_secs(1)))["cpu"],
        Amostra::SemLeitura
    );
}

#[test]
fn cpu_por_uso_calcula_fracao_ocupada() {
    let t = tempfile::tempdir().unwrap();
    // user nice system idle iowait irq softirq steal
    escrever(
        t.path(),
        "proc/stat",
        "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 1 2 3\n",
    );
    let mut m = CpuPorUso::novo(&RaizDoSistema::em(t.path().to_path_buf()));
    escrever(t.path(), "proc/stat", "cpu  200 0 200 900 0 0 0 0 0 0\n");
    let l = mapa(m.ler(Duration::from_secs(1)));
    let Amostra::Atividade(uso) = l["cpu"] else {
        panic!("esperava atividade")
    };
    assert!((uso - 2.0 / 3.0).abs() < 1e-12);
}
