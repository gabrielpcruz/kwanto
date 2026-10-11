//! Item de bandeja pelo protocolo StatusNotifierItem (KDE, GNOME com AppIndicator, XFCE, Cinnamon…).
//! Implementado direto com zbus para ter o rótulo "123 W" (XAyatanaLabel) ao lado do ícone.
use crate::aparencia::Aparencia;
use crate::icones::{Icones, Pixmap};
use crate::{Avisar, Evento};
use futures_util::StreamExt as _;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedObjectPath;

pub const CAMINHO_ITEM: &str = "/StatusNotifierItem";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const GUIA_ROTULO: &str = "0000 W";

pub struct ItemBandeja {
    aparencia: Aparencia,
    icones: Icones,
    avisar: Avisar,
}

impl std::fmt::Debug for ItemBandeja {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ItemBandeja")
            .field("aparencia", &self.aparencia)
            .finish_non_exhaustive()
    }
}

impl ItemBandeja {
    #[must_use]
    pub fn novo(icones: Icones, avisar: Avisar) -> Self {
        Self {
            aparencia: Aparencia::de_status(None),
            icones,
            avisar,
        }
    }

    fn clique(&self, x: i32, y: i32) {
        (self.avisar)(Evento::Alternar { x, y });
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl ItemBandeja {
    fn activate(&self, x: i32, y: i32) {
        self.clique(x, y);
    }
    fn secondary_activate(&self, x: i32, y: i32) {
        self.clique(x, y);
    }
    fn context_menu(&self, x: i32, y: i32) {
        self.clique(x, y);
    }
    fn scroll(&self, _delta: i32, _orientacao: &str) {}

    #[zbus(property(emits_changed_signal = "false"))]
    fn category(&self) -> &str {
        "Hardware"
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn id(&self) -> &str {
        "kwanto"
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn title(&self) -> &str {
        "Kwanto"
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn status(&self) -> &str {
        "Active"
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn window_id(&self) -> i32 {
        0
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn icon_name(&self) -> &str {
        ""
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn icon_theme_path(&self) -> &str {
        ""
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn icon_pixmap(&self) -> Vec<Pixmap> {
        self.icones.para(self.aparencia.ativo)
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn attention_icon_name(&self) -> &str {
        ""
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn overlay_icon_name(&self) -> &str {
        ""
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn tool_tip(&self) -> (String, Vec<Pixmap>, String, String) {
        (
            String::new(),
            Vec::new(),
            self.aparencia.dica_titulo.clone(),
            self.aparencia.dica_texto.clone(),
        )
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn item_is_menu(&self) -> bool {
        false
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from("/NO_DBUSMENU").unwrap_or_default()
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn x_ayatana_label(&self) -> &str {
        &self.aparencia.rotulo
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn x_ayatana_label_guide(&self) -> &str {
        GUIA_ROTULO
    }

    #[zbus(signal)]
    async fn new_icon(emissor: &SignalEmitter<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn new_tool_tip(emissor: &SignalEmitter<'_>) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn x_ayatana_new_label(
        emissor: &SignalEmitter<'_>,
        rotulo: &str,
        guia: &str,
    ) -> zbus::Result<()>;
}

/// Publica o item no barramento com o nome que a especificação SNI sugere; devolve esse nome.
pub async fn publicar(conn: &zbus::Connection, item: ItemBandeja) -> zbus::Result<String> {
    let nome = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    conn.object_server().at(CAMINHO_ITEM, item).await?;
    conn.request_name(nome.as_str()).await?;
    Ok(nome)
}

/// Troca o que o ícone mostra e avisa a bandeja só do que mudou.
pub async fn atualizar(conn: &zbus::Connection, nova: Aparencia) -> zbus::Result<()> {
    let iface = conn
        .object_server()
        .interface::<_, ItemBandeja>(CAMINHO_ITEM)
        .await?;
    let anterior = {
        let mut item = iface.get_mut().await;
        std::mem::replace(&mut item.aparencia, nova.clone())
    };
    let emissor = iface.signal_emitter();
    if anterior.rotulo != nova.rotulo {
        ItemBandeja::x_ayatana_new_label(emissor, &nova.rotulo, GUIA_ROTULO).await?;
    }
    if anterior.ativo != nova.ativo {
        ItemBandeja::new_icon(emissor).await?;
    }
    if anterior.dica_texto != nova.dica_texto || anterior.dica_titulo != nova.dica_titulo {
        ItemBandeja::new_tool_tip(emissor).await?;
    }
    Ok(())
}

async fn registrar(conn: &zbus::Connection, nome: &str) -> zbus::Result<()> {
    conn.call_method(
        Some(WATCHER),
        "/StatusNotifierWatcher",
        Some(WATCHER),
        "RegisterStatusNotifierItem",
        &(nome,),
    )
    .await
    .map(|_| ())
}

/// Registra o item agora (se houver bandeja) e de novo toda vez que a bandeja (re)aparecer.
pub async fn manter_registrado(conn: zbus::Connection, nome: String) {
    let dbus = match zbus::fdo::DBusProxy::new(&conn).await {
        Ok(p) => p,
        Err(e) => return tracing::error!(erro = %e, "sem acesso ao D-Bus"),
    };
    let mut mudancas = match dbus.receive_name_owner_changed().await {
        Ok(s) => s,
        Err(e) => return tracing::error!(erro = %e, "não foi possível acompanhar a bandeja"),
    };
    if registrar(&conn, &nome).await.is_err() {
        tracing::warn!(
            "nenhuma bandeja do sistema encontrada; no GNOME, ative a extensão \
             \"AppIndicator and KStatusNotifierItem Support\". O ícone aparece quando ela estiver ativa."
        );
    }
    while let Some(sinal) = mudancas.next().await {
        let Ok(args) = sinal.args() else { continue };
        if args.name().as_str() != WATCHER || args.new_owner().is_none() {
            continue;
        }
        if let Err(e) = registrar(&conn, &nome).await {
            tracing::warn!(erro = %e, "a bandeja apareceu, mas recusou o registro");
        }
    }
}

/// A bandeja real: atualiza o item publicado nesta conexão.
#[derive(Debug, Clone)]
pub struct BandejaSni(pub zbus::Connection);

#[async_trait::async_trait]
impl crate::monitor::Bandeja for BandejaSni {
    async fn mostrar(&self, aparencia: Aparencia) {
        if let Err(e) = atualizar(&self.0, aparencia).await {
            tracing::warn!(erro = %e, "não foi possível atualizar o ícone");
        }
    }
}
