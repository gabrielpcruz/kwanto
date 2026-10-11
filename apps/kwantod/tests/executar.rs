//! Sobe o serviço completo com hardware falso, numa porta livre, e confere a API e o desligamento.
use async_trait::async_trait;
use kwanto_app::portas::{Intervalo, RepositorioEnergia};
use kwanto_core::componente::{Amostra, Componente, ComponenteId, Perfil};
use kwanto_core::relogio::RelogioFalso;
use kwanto_core::sensores::{DetectorDeHardware, Hardware, Medidor};
use kwanto_core::unidades::Watts;
use kwanto_ext::{EventoDeDominio, ExtensaoColeta};
use kwantod::{Extensoes, Plataforma, executar};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

struct Gpu;
impl Medidor for Gpu {
    fn componentes(&self) -> Vec<Componente> {
        vec![Componente::novo(
            "gpu:x",
            "GPU de teste",
            "NVML (driver NVIDIA)",
            Perfil::Sensor,
        )]
    }
    fn ler(&mut self, _dt: Duration) -> Vec<(ComponenteId, Amostra)> {
        vec![(ComponenteId::new("gpu:x"), Amostra::Potencia(Watts(50.0)))]
    }
}

struct Detector;
impl DetectorDeHardware for Detector {
    fn detectar(&self) -> Hardware {
        Hardware {
            medidores: vec![Box::new(Gpu)],
            cpu_estimada: false,
            tem_hdd: false,
            notebook: false,
        }
    }
}

struct Contador(Arc<AtomicUsize>);
#[async_trait]
impl ExtensaoColeta for Contador {
    fn nome(&self) -> &str {
        "teste"
    }
    async fn ao_evento(&self, _e: &EventoDeDominio) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn servico_completo_responde_e_desliga_gravando() {
    let pasta = tempfile::tempdir().unwrap();
    let porta = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let relogio = Arc::new(RelogioFalso::novo(jiff::Zoned::now()));
    let plataforma = Plataforma {
        detector: Box::new(Detector),
        relogio: relogio.clone(),
        banco: pasta.path().join("k.db"),
        endereco: format!("127.0.0.1:{porta}").parse().unwrap(),
    };
    let eventos = Arc::new(AtomicUsize::new(0));
    let ext = Extensoes {
        coleta: vec![Arc::new(Contador(eventos.clone()))],
        http: vec![],
    };
    let (parar_tx, parar_rx) = tokio::sync::oneshot::channel::<()>();
    let servico = tokio::spawn(executar(plataforma, ext, async move {
        let _ = parar_rx.await;
    }));

    // o amostrador real dorme 2 s entre leituras; avançamos o relógio falso junto
    let url = format!("http://127.0.0.1:{porta}");
    let cliente = reqwest::Client::new();
    let mut ok = false;
    for _ in 0..20 {
        relogio.avancar(Duration::from_secs(2));
        tokio::time::sleep(Duration::from_millis(2100)).await;
        if let Ok(r) = cliente.get(format!("{url}/api/status")).send().await {
            let s: serde_json::Value = r.json().await.unwrap();
            if s["agora"]["parede_w"].as_f64().unwrap_or(0.0) > 50.0 {
                ok = true;
                break;
            }
        }
    }
    assert!(ok, "a API não mostrou a leitura da GPU");
    parar_tx.send(()).unwrap();
    servico.await.unwrap().unwrap();
    assert!(
        eventos.load(Ordering::SeqCst) >= 1,
        "o minuto parcial deveria ser publicado ao desligar"
    );
    // e precisa estar no banco: a energia da GPU de teste (50 W) foi gravada
    let banco = kwanto_store::ArmazemSqlite::abrir(&pasta.path().join("k.db"), &[]).unwrap();
    let totais = banco
        .totais_por_componente(Intervalo {
            inicio: 0,
            fim: i64::MAX,
        })
        .unwrap();
    let gpu = totais
        .get(&ComponenteId::new("gpu:x"))
        .copied()
        .unwrap_or_default();
    assert!(gpu.0 > 0.0, "o minuto parcial não chegou ao banco");
}

struct GpuQuebra(u32);
impl Medidor for GpuQuebra {
    fn componentes(&self) -> Vec<Componente> {
        Gpu.componentes()
    }
    fn ler(&mut self, dt: Duration) -> Vec<(ComponenteId, Amostra)> {
        self.0 += 1;
        assert!(self.0 < 2, "defeito inesperado no medidor");
        Gpu.ler(dt)
    }
}

struct DetectorQuebrado;
impl DetectorDeHardware for DetectorQuebrado {
    fn detectar(&self) -> Hardware {
        Hardware {
            medidores: vec![Box::new(GpuQuebra(0))],
            cpu_estimada: false,
            tem_hdd: false,
            notebook: false,
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn coleta_que_quebra_encerra_o_servico_com_erro() {
    let pasta = tempfile::tempdir().unwrap();
    let porta = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let relogio = Arc::new(RelogioFalso::novo(jiff::Zoned::now()));
    let plataforma = Plataforma {
        detector: Box::new(DetectorQuebrado),
        relogio: relogio.clone(),
        banco: pasta.path().join("k.db"),
        endereco: format!("127.0.0.1:{porta}").parse().unwrap(),
    };
    // nunca pede para desligar: só a falha da coleta pode encerrar
    let servico = tokio::spawn(executar(
        plataforma,
        Extensoes::default(),
        std::future::pending(),
    ));
    let avancar = tokio::spawn(async move {
        loop {
            relogio.avancar(Duration::from_secs(2));
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });
    let resultado = tokio::time::timeout(Duration::from_secs(15), servico).await;
    avancar.abort();
    let resultado = resultado
        .expect("o serviço continuou no ar com a coleta morta")
        .unwrap();
    assert!(
        resultado.is_err(),
        "deveria terminar com erro para o systemd reiniciar"
    );
}
