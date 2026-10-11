//! Bandeja do Kwanto. Thread principal: janela (tao/wry). Runtime tokio: D-Bus e monitor.
use kwanto::icones::Icones;
use kwanto::instancia::tentar_ser_unica;
use kwanto::monitor::{Servico, UltimoStatus, acompanhar, donos_confiaveis};
use kwanto::ponte::MensagemPopup;
use kwanto::popup::Popup;
use kwanto::sni::{BandejaSni, ItemBandeja, manter_registrado, publicar};
use kwanto::{Avisar, Evento};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::time::Instant;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

/// O renderizador DMA-BUF e o modo de composição do WebKitGTK deixam a janela vazia com o driver
/// proprietário da NVIDIA. O popup é pequeno; renderizar sem GPU não faz diferença.
const CHECAGEM_POPUP: Duration = Duration::from_secs(3);

fn preparar_webkit() {
    for var in [
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
    ] {
        if std::env::var_os(var).is_none() {
            // SAFETY: chamado no início do main, antes de criar qualquer thread.
            #[allow(unsafe_code)]
            unsafe {
                std::env::set_var(var, "1");
            }
        }
    }
}

fn main() {
    if let Err(e) = iniciar() {
        eprintln!("Kwanto: {e:#}");
        std::process::exit(1);
    }
}

fn iniciar() -> anyhow::Result<()> {
    preparar_webkit();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    // KWANTO_URL (desenvolvimento): o próprio usuário passa a ser dono aceito do serviço.
    let url_explicita = std::env::var("KWANTO_URL").ok();
    let url = url_explicita
        .clone()
        .unwrap_or_else(|| "http://127.0.0.1:8787".into())
        .trim_end_matches('/')
        .to_string();
    let servico = Servico::da_url(&url, donos_confiaveis(url_explicita.is_some()));
    if servico.is_none() {
        tracing::warn!(%url, "endereço do serviço inválido: use http://127.0.0.1:<porta>");
    }
    let ultimo = UltimoStatus::default();

    let event_loop = EventLoopBuilder::<Evento>::with_user_event().build();
    // O proxy do tao é Send mas não Sync; o Mutex permite chamá-lo de várias threads.
    let proxy = Mutex::new(event_loop.create_proxy());
    let avisar: Avisar = Arc::new(move |e| {
        if let Ok(p) = proxy.lock() {
            let _ = p.send_event(e);
        }
    });

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let conn = rt.block_on(zbus::Connection::session())?;
    if !rt.block_on(tentar_ser_unica(&conn, avisar.clone()))? {
        tracing::info!("o Kwanto já está na bandeja; mostrei o popup dele");
        return Ok(());
    }
    let nome = rt.block_on(publicar(
        &conn,
        ItemBandeja::novo(Icones::embutidos(), avisar.clone()),
    ))?;
    rt.spawn(manter_registrado(conn.clone(), nome));
    let bandeja = Arc::new(BandejaSni(conn.clone()));
    rt.spawn(acompanhar(
        servico,
        ultimo.clone(),
        bandeja,
        avisar.clone(),
        Duration::from_secs(5),
    ));

    rt.spawn(kwanto::atualizador::ciclo(conn.clone(), avisar.clone()));

    let mut popup = Popup::novo(&event_loop, ultimo, avisar)?;
    event_loop.run(move |evento, _, controle| {
        // Acorda a cada 3 s para recarregar o popup se ele não tiver carregado.
        *controle = ControlFlow::WaitUntil(Instant::now() + CHECAGEM_POPUP);
        let _manter_vivos = (&rt, &conn);
        match evento {
            Event::UserEvent(Evento::Alternar { x, y }) => popup.alternar(Some((x, y))),
            Event::UserEvent(Evento::Mostrar) => popup.mostrar(None),
            Event::UserEvent(Evento::Servico(ok)) => popup.servico(ok),
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => popup.checar_carga(),
            Event::UserEvent(Evento::Popup(m)) => {
                popup.mensagem_recebida();
                match m {
                    MensagemPopup::Altura(h) => popup.ajustar_altura(h),
                    MensagemPopup::Abrir => {
                        if popup.servico_ok() {
                            let _ = open::that_detached(format!("{url}/"));
                        }
                        popup.esconder();
                    }
                    MensagemPopup::Fechar => popup.esconder(),
                    MensagemPopup::Sair => *controle = ControlFlow::Exit,
                }
            }
            Event::UserEvent(Evento::ReiniciarBandeja) => {
                use std::os::unix::process::CommandExt as _;
                // exec troca este processo pelo binário novo; o D-Bus fecha junto e libera os nomes.
                let erro = std::process::Command::new("/usr/bin/kwanto").exec();
                tracing::error!(erro = %erro, "não foi possível reiniciar a bandeja");
            }
            Event::WindowEvent {
                event: WindowEvent::Focused(false),
                ..
            } => popup.perdeu_foco(),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => popup.esconder(),
            _ => {}
        }
    });
}
