//! Serviço do Kwanto: compõe sensores, coleta, banco e API. É uma biblioteca para que outro binário
//! possa compor o mesmo serviço passando as próprias extensões.
mod porteiro;
mod relogio_sistema;

pub use relogio_sistema::RelogioSistema;

use anyhow::Context as _;
use kwanto_api::{Contexto, EstadoApi, ExtensaoHttp, rotas};
use kwanto_app::coleta::{Amostrador, EstadoAoVivo, INTERVALO_AMOSTRA};
use kwanto_app::config_uc::{AtualizarConfig, carregar_config};
use kwanto_app::historico::ConsultarHistorico;
use kwanto_app::portas::CatalogoComponentes;
use kwanto_app::registrar::RegistrarMinuto;
use kwanto_app::status::{ConsultarStatus, InfoHardware};
use kwanto_core::eventos::Migracao;
use kwanto_core::relogio::Relogio;
use kwanto_core::sensores::DetectorDeHardware;
use kwanto_ext::{ExtensaoColeta, despachar};
use kwanto_sensors::detector::DetectorLinux;
use kwanto_sensors::raiz::RaizDoSistema;
use kwanto_store::ArmazemSqlite;
use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::{broadcast, watch};

pub struct Plataforma {
    pub detector: Box<dyn DetectorDeHardware>,
    pub relogio: Arc<dyn Relogio>,
    pub banco: PathBuf,
    pub endereco: SocketAddr,
}

impl std::fmt::Debug for Plataforma {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Plataforma")
            .field("banco", &self.banco)
            .field("endereco", &self.endereco)
            .finish_non_exhaustive()
    }
}

#[derive(Default)]
pub struct Extensoes {
    pub coleta: Vec<Arc<dyn ExtensaoColeta>>,
    pub http: Vec<Arc<dyn ExtensaoHttp>>,
}

impl std::fmt::Debug for Extensoes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Extensoes")
            .field("coleta", &self.coleta.len())
            .field("http", &self.http.len())
            .finish()
    }
}

/// Plataforma Linux real, lida das variáveis KWANTO_DB, KWANTO_HOST e KWANTO_PORTA.
pub fn plataforma_linux() -> anyhow::Result<Plataforma> {
    let banco = std::env::var("KWANTO_DB").unwrap_or_else(|_| "/var/lib/kwanto/kwanto.db".into());
    let host = std::env::var("KWANTO_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let porta = std::env::var("KWANTO_PORTA").unwrap_or_else(|_| "8787".into());
    let endereco = format!("{host}:{porta}")
        .parse()
        .with_context(|| format!("endereço inválido: {host}:{porta}"))?;
    Ok(Plataforma {
        detector: Box::new(DetectorLinux::novo(RaizDoSistema::real())),
        relogio: Arc::new(RelogioSistema),
        banco: banco.into(),
        endereco,
    })
}

/// Termina com SIGTERM (systemd) ou Ctrl+C.
pub async fn sinal_de_desligar() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = ctrl_c => {}
                    _ = term.recv() => {}
                }
            }
            Err(_) => {
                let _ = ctrl_c.await;
            }
        }
    }
    #[cfg(not(unix))]
    let _ = ctrl_c.await;
}

pub async fn executar(
    plataforma: Plataforma,
    extensoes: Extensoes,
    desligar: impl Future<Output = ()> + Send + 'static,
) -> anyhow::Result<()> {
    // banco (núcleo + migrações das extensões)
    let migracoes: Vec<(&str, &[Migracao])> = extensoes
        .coleta
        .iter()
        .map(|e| (e.nome(), e.migracoes()))
        .collect();
    let armazem = Arc::new(
        ArmazemSqlite::abrir(&plataforma.banco, &migracoes).with_context(|| {
            format!(
                "não foi possível abrir o banco {}",
                plataforma.banco.display()
            )
        })?,
    );

    // hardware
    let hardware = plataforma.detector.detectar();
    let info = InfoHardware {
        cpu_estimada: hardware.cpu_estimada,
        tem_hdd: hardware.tem_hdd,
    };
    let descricoes: Vec<_> = hardware
        .medidores
        .iter()
        .flat_map(|m| m.componentes())
        .map(|c| c.descricao())
        .collect();
    for d in &descricoes {
        tracing::info!(tipo = d.tipo.como_texto(), componente = %d.rotulo, origem = %d.origem, "componente detectado");
    }
    armazem.registrar(&descricoes)?;

    // estado compartilhado
    let config = carregar_config(armazem.as_ref(), hardware.notebook)?;
    let (config_tx, config_rx) = watch::channel(config);
    let (estado_tx, estado_rx) = watch::channel(Arc::new(EstadoAoVivo::default()));
    let (eventos_tx, eventos_rx) = broadcast::channel(256);
    let despacho = despachar(extensoes.coleta, eventos_rx);

    // coleta numa thread própria (leituras de sensores bloqueiam)
    let parar = Arc::new(AtomicBool::new(false));
    let relogio = Arc::clone(&plataforma.relogio);
    let mut amostrador = Amostrador::novo(
        hardware.medidores,
        Arc::clone(&relogio),
        config_rx.clone(),
        estado_tx,
    );
    let mut registrar = RegistrarMinuto::novo(armazem.clone(), eventos_tx.clone());
    let parar_coleta = Arc::clone(&parar);
    // Se a coleta morrer, o serviço encerra com erro para o systemd reiniciá-lo,
    // em vez de ficar no ar servindo números parados.
    let (coleta_morreu_tx, coleta_morreu) = tokio::sync::oneshot::channel::<()>();
    let coleta = std::thread::Builder::new()
        .name("kwanto-coleta".into())
        .spawn(move || {
            let resultado = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                while !parar_coleta.load(Ordering::SeqCst) {
                    std::thread::sleep(INTERVALO_AMOSTRA);
                    if let Some(r) = amostrador.passo() {
                        registrar.executar(r);
                    }
                }
                if let Some(r) = amostrador.encerrar() {
                    registrar.executar(r); // minuto parcial gravado no desligamento
                }
            }));
            let falhou = resultado.is_err();
            if falhou {
                let _ = coleta_morreu_tx.send(());
            }
            falhou
        })?;

    // API
    let estado_api = EstadoApi {
        status: Arc::new(ConsultarStatus::novo(
            armazem.clone(),
            armazem.clone(),
            Arc::clone(&relogio),
            config_rx.clone(),
            estado_rx.clone(),
            info,
        )),
        historico: Arc::new(ConsultarHistorico::novo(
            armazem.clone(),
            relogio,
            config_rx.clone(),
        )),
        config: Arc::new(AtualizarConfig::novo(
            armazem.clone(),
            config_tx,
            eventos_tx,
        )),
        paineis: Arc::new(extensoes.http.iter().flat_map(|e| e.paineis()).collect()),
    };
    let ctx = Contexto {
        energia: armazem.clone(),
        catalogo: armazem,
        config: config_rx,
        estado: estado_rx,
    };
    let porteiro = Arc::new(porteiro::PorteiroSistema::novo(
        plataforma.endereco,
        Arc::new(porteiro::Logind::conectar().await),
    ));
    let app = rotas(
        estado_api,
        plataforma.endereco.port(),
        &extensoes.http,
        ctx,
        porteiro,
    );
    let ouvinte = tokio::net::TcpListener::bind(plataforma.endereco).await.with_context(|| {
        format!(
            "não foi possível abrir {}. Outro programa usa a porta? Defina outra com KWANTO_PORTA.",
            plataforma.endereco
        )
    })?;
    tracing::info!(endereco = %plataforma.endereco, "dashboard disponível");
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Ready]);

    let encerrar = async move {
        tokio::select! {
            () = desligar => {}
            Ok(()) = coleta_morreu => tracing::error!("a coleta parou por um erro inesperado; encerrando o serviço"),
        }
    };
    // O endereço de cada conexão é o que o porteiro usa para saber quem está chamando.
    axum::serve(
        ouvinte,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(encerrar)
    .await?;

    // desligamento ordenado
    tracing::info!("desligando: gravando o minuto parcial");
    parar.store(true, Ordering::SeqCst);
    let coleta_falhou = tokio::task::spawn_blocking(move || coleta.join())
        .await?
        .map_err(|_| anyhow::anyhow!("a thread de coleta terminou com pânico"))?;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await; // deixa o despacho entregar o último evento
    despacho.abort();
    if coleta_falhou {
        anyhow::bail!("a coleta parou por um erro inesperado; o systemd vai reiniciar o serviço");
    }
    Ok(())
}
