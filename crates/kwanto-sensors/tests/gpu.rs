use kwanto_core::componente::Amostra;
use kwanto_core::sensores::Medidor;
use kwanto_core::unidades::Watts;
use kwanto_sensors::hwmon::GpusHwmon;
use kwanto_sensors::nvidia::{ColetorNvidia, FonteNvidia, InfoGpu, ler_cache};
use kwanto_sensors::raiz::RaizDoSistema;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

fn escrever(base: &Path, rel: &str, conteudo: &str) {
    let p = base.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, conteudo).unwrap();
}

/// Cria card<N>/device como link para o diretório PCI (o detector resolve o nome PCI pelo destino).
fn placa(base: &Path, card: &str, pci: &str, vendor: &str, extra: &[(&str, &str)]) {
    let dev = format!("sys/bus/pci/devices/{pci}");
    escrever(base, &format!("{dev}/vendor"), vendor);
    escrever(base, &format!("{dev}/power/runtime_status"), "active");
    for (rel, v) in extra {
        escrever(base, &format!("{dev}/{rel}"), v);
    }
    fs::create_dir_all(base.join("sys/class/drm").join(card)).unwrap();
    std::os::unix::fs::symlink(
        base.join(&dev),
        base.join("sys/class/drm").join(card).join("device"),
    )
    .unwrap();
}

#[test]
fn hwmon_amd_dedicada_le_power_average_e_ignora_igpu() {
    let t = tempfile::tempdir().unwrap();
    placa(
        t.path(),
        "card0",
        "0000:03:00.0",
        "0x1002",
        &[
            ("mem_info_vram_total", "8589934592"),
            ("hwmon/hwmon5/power1_average", "45000000"),
        ],
    );
    placa(
        t.path(),
        "card1",
        "0000:00:02.0",
        "0x8086",
        &[("hwmon/hwmon6/power1_input", "9000000")],
    );
    let mut m = GpusHwmon::detectar(&RaizDoSistema::em(t.path().to_path_buf())).expect("gpu");
    let c = m.componentes();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].id.as_str(), "gpu:0000:03:00.0");
    assert_eq!(
        m.ler(Duration::from_secs(2))[0].1,
        Amostra::Potencia(Watts(45.0))
    );
}

#[test]
fn hwmon_gpu_dormindo_reporta_zero_sem_ler_sensor() {
    let t = tempfile::tempdir().unwrap();
    placa(
        t.path(),
        "card0",
        "0000:03:00.0",
        "0x1002",
        &[
            ("mem_info_vram_total", "8589934592"),
            ("hwmon/hwmon5/power1_input", "45000000"),
        ],
    );
    escrever(
        t.path(),
        "sys/bus/pci/devices/0000:03:00.0/power/runtime_status",
        "suspended",
    );
    let mut m = GpusHwmon::detectar(&RaizDoSistema::em(t.path().to_path_buf())).unwrap();
    assert_eq!(
        m.ler(Duration::from_secs(2))[0].1,
        Amostra::Potencia(Watts(0.0))
    );
}

#[test]
fn hwmon_por_contador_de_energia() {
    let t = tempfile::tempdir().unwrap();
    placa(
        t.path(),
        "card0",
        "0000:05:00.0",
        "0x8086",
        &[("hwmon/hwmon1/energy1_input", "1000000")],
    );
    let mut m = GpusHwmon::detectar(&RaizDoSistema::em(t.path().to_path_buf())).unwrap();
    escrever(
        t.path(),
        "sys/bus/pci/devices/0000:05:00.0/hwmon/hwmon1/energy1_input",
        "21000000",
    );
    assert_eq!(
        m.ler(Duration::from_secs(2))[0].1,
        Amostra::Potencia(Watts(10.0))
    );
}

struct FonteFalsa {
    falhar: bool,
}
impl FonteNvidia for FonteFalsa {
    fn listar(&mut self) -> Result<Vec<InfoGpu>, String> {
        Ok(vec![InfoGpu {
            uuid: "GPU-abc".into(),
            nome: "NVIDIA GeForce RTX 4060".into(),
            pci: "0000:0a:00.0".into(),
        }])
    }
    fn potencia(&mut self, _uuid: &str) -> Result<Watts, String> {
        if self.falhar {
            Err("Driver/library version mismatch".into())
        } else {
            Ok(Watts(61.5))
        }
    }
}

#[test]
fn nvidia_rodada_preenche_cache_e_falha_vira_sem_leitura() {
    let t = tempfile::tempdir().unwrap();
    escrever(
        t.path(),
        "sys/bus/pci/devices/0000:0a:00.0/power/runtime_status",
        "active",
    );
    let raiz = RaizDoSistema::em(t.path().to_path_buf());
    let mut fonte = FonteFalsa { falhar: false };
    let gpus = fonte.listar().unwrap();
    let mut col = ColetorNvidia::novo(Box::new(fonte), raiz.clone(), gpus.clone());
    col.rodada();
    let cache = col.cache();
    assert_eq!(
        ler_cache(&cache, "GPU-abc", Instant::now()),
        Amostra::Potencia(Watts(61.5))
    );
    // leitura velha (mais de 10 s) vira SemLeitura
    let futuro = Instant::now() + Duration::from_secs(11);
    assert_eq!(ler_cache(&cache, "GPU-abc", futuro), Amostra::SemLeitura);

    let mut col = ColetorNvidia::novo(Box::new(FonteFalsa { falhar: true }), raiz, gpus);
    col.rodada();
    assert_eq!(
        ler_cache(&col.cache(), "GPU-abc", Instant::now()),
        Amostra::SemLeitura
    );
}

#[test]
fn nvidia_dormindo_reporta_zero_sem_consultar() {
    let t = tempfile::tempdir().unwrap();
    escrever(
        t.path(),
        "sys/bus/pci/devices/0000:0a:00.0/power/runtime_status",
        "suspended",
    );
    let gpus = FonteFalsa { falhar: true }.listar().unwrap();
    let raiz = RaizDoSistema::em(t.path().to_path_buf());
    let mut col = ColetorNvidia::novo(Box::new(FonteFalsa { falhar: true }), raiz, gpus);
    col.rodada();
    assert_eq!(
        ler_cache(&col.cache(), "GPU-abc", Instant::now()),
        Amostra::Potencia(Watts(0.0))
    );
}

/// Fonte que conta leituras e liberações e informa se a placa está ocupada.
struct FonteContadora {
    liberacoes: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    leituras: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ocupada: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl FonteNvidia for FonteContadora {
    fn listar(&mut self) -> Result<Vec<InfoGpu>, String> {
        FonteFalsa { falhar: false }.listar()
    }
    fn potencia(&mut self, _uuid: &str) -> Result<Watts, String> {
        self.leituras
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(Watts(30.0))
    }
    fn ocupada(&mut self, _uuid: &str) -> Result<bool, String> {
        Ok(self.ocupada.load(std::sync::atomic::Ordering::SeqCst))
    }
    fn liberar(&mut self) {
        self.liberacoes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

struct Cenario {
    _t: tempfile::TempDir,
    base: std::path::PathBuf,
    col: ColetorNvidia,
    liberacoes: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    leituras: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ocupada: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Cenario {
    fn novo(controle: &str, boot_vga: Option<&str>) -> Self {
        let t = tempfile::tempdir().unwrap();
        let base = t.path().to_path_buf();
        let dev = "sys/bus/pci/devices/0000:0a:00.0";
        escrever(&base, &format!("{dev}/power/runtime_status"), "active");
        escrever(&base, &format!("{dev}/power/control"), controle);
        if let Some(v) = boot_vga {
            escrever(&base, &format!("{dev}/boot_vga"), v);
        }
        let liberacoes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let leituras = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let ocupada = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let fonte = FonteContadora {
            liberacoes: liberacoes.clone(),
            leituras: leituras.clone(),
            ocupada: ocupada.clone(),
        };
        let gpus = FonteFalsa { falhar: false }.listar().unwrap();
        let col = ColetorNvidia::novo(Box::new(fonte), RaizDoSistema::em(base.clone()), gpus);
        Self {
            _t: t,
            base,
            col,
            liberacoes,
            leituras,
            ocupada,
        }
    }
    fn estado(&self, s: &str) {
        escrever(
            &self.base,
            "sys/bus/pci/devices/0000:0a:00.0/power/runtime_status",
            s,
        );
    }
    fn liberacoes(&self) -> usize {
        self.liberacoes.load(std::sync::atomic::Ordering::SeqCst)
    }
    fn leituras(&self) -> usize {
        self.leituras.load(std::sync::atomic::Ordering::SeqCst)
    }
    /// `n` rodadas de 2 s a partir de `inicio`; devolve o instante seguinte.
    fn rodadas(&mut self, inicio: Instant, n: u32) -> Instant {
        let mut t = inicio;
        for _ in 0..n {
            self.col.rodada_em(t);
            t += Duration::from_secs(2);
        }
        t
    }
}

/// Desktop: a GPU principal (com a tela) nunca dorme; liberar o driver a cada leitura não adianta e
/// vaza recursos do driver NVIDIA (um descritor por ciclo, até o serviço parar).
#[test]
fn gpu_principal_nunca_e_liberada() {
    let mut c = Cenario::novo("auto", Some("1"));
    c.ocupada.store(false, std::sync::atomic::Ordering::SeqCst);
    c.rodadas(Instant::now(), 1_000);
    assert_eq!(
        c.liberacoes(),
        0,
        "ociosa e com economia automática, mas é a principal"
    );
    let mut c = Cenario::novo("on", Some("0"));
    c.rodadas(Instant::now(), 100);
    assert_eq!(c.liberacoes(), 0, "sem economia automática, nunca libera");
}

/// Híbrido: enquanto a placa trabalha, o driver fica aberto (nada de liberar e reabrir a cada 2 s).
#[test]
fn gpu_dedicada_em_uso_fica_aberta() {
    let mut c = Cenario::novo("auto", Some("0"));
    c.rodadas(Instant::now(), 1_000);
    assert_eq!(c.liberacoes(), 0);
}

/// Híbrido: placa ociosa é liberada para dormir; dormindo, nem é consultada.
#[test]
fn gpu_dedicada_ociosa_e_liberada_e_dorme() {
    let mut c = Cenario::novo("auto", Some("0"));
    c.ocupada.store(false, std::sync::atomic::Ordering::SeqCst);
    let t = c.rodadas(Instant::now(), 3);
    assert_eq!(c.liberacoes(), 1, "três leituras ociosas seguidas");
    c.estado("suspended");
    let antes = c.leituras();
    c.rodadas(t, 500);
    assert_eq!(c.leituras(), antes, "dormindo: não acorda a placa");
    assert_eq!(c.liberacoes(), 1);
}

/// Híbrido: liberada, mas outro programa a mantém acordada. Volta a segurar e só tenta de novo
/// depois de esperas crescentes: poucos ciclos por hora, não um a cada 2 s.
#[test]
fn gpu_que_nao_dorme_espera_cada_vez_mais() {
    let mut c = Cenario::novo("auto", Some("0"));
    c.ocupada.store(false, std::sync::atomic::Ordering::SeqCst);
    let leituras_antes = c.leituras();
    let t = c.rodadas(Instant::now(), 3);
    assert_eq!(c.liberacoes(), 1);
    // logo depois de liberar, não reabre o driver (senão a placa nunca dormiria)
    let t = c.rodadas(t, 10);
    assert_eq!(
        c.leituras(),
        leituras_antes + 3,
        "não consulta durante a espera para dormir"
    );
    // uma hora com a placa acordada por outro programa
    c.rodadas(t, 1_800);
    let n = c.liberacoes();
    assert!(
        (2..=8).contains(&n),
        "{n} liberações em 1 h (esperas crescentes)"
    );
}

#[test]
fn nvidia_tenta_de_novo_enquanto_o_driver_nao_carrega() {
    use kwanto_sensors::detector::iniciar_nvidia;
    let t = tempfile::tempdir().unwrap();
    escrever(
        t.path(),
        "sys/bus/pci/devices/0000:0a:00.0/runtime_status",
        "active",
    );
    let raiz = RaizDoSistema::em(t.path().to_path_buf());
    let mut chamadas = 0;
    let g = iniciar_nvidia(
        &raiz,
        || {
            chamadas += 1;
            if chamadas < 3 {
                Err("driver não carregado".into())
            } else {
                Ok(Box::new(FonteFalsa { falhar: false }) as Box<dyn FonteNvidia>)
            }
        },
        5,
        Duration::ZERO,
    );
    assert!(g.is_some());
    assert_eq!(chamadas, 3);
    let mut chamadas = 0;
    let g = iniciar_nvidia(
        &raiz,
        || {
            chamadas += 1;
            Err("sem driver".into())
        },
        4,
        Duration::ZERO,
    );
    assert!(g.is_none());
    assert_eq!(chamadas, 4);
}

/// O ID exposto não revela o UUID da placa, e a leitura continua chegando pelo mesmo ID.
#[test]
fn nvidia_expoe_id_sem_o_uuid() {
    let t = tempfile::tempdir().unwrap();
    let raiz = RaizDoSistema::em(t.path().to_path_buf());
    let mut g =
        kwanto_sensors::nvidia::GpusNvidia::com_fonte(Box::new(FonteFalsa { falhar: false }), raiz)
            .unwrap();
    let c = g.componentes();
    assert_eq!(
        c[0].id,
        kwanto_core::componente::ComponenteId::anonimo("gpu", "GPU-abc")
    );
    assert!(!c[0].id.as_str().contains("GPU-abc"));
    assert_eq!(g.ler(Duration::from_secs(2))[0].0, c[0].id);
}
