mod comum;

use comum::{BarramentoPrivado, esperar, subir_watcher};
use kwanto::aparencia::Aparencia;
use kwanto::icones::Icones;
use kwanto::sni::{CAMINHO_ITEM, ItemBandeja, atualizar, manter_registrado, publicar};
use kwanto::{Avisar, Evento};
use std::sync::{Arc, Mutex};

fn coletor() -> (Avisar, Arc<Mutex<Vec<Evento>>>) {
    let eventos = Arc::new(Mutex::new(Vec::new()));
    let e = eventos.clone();
    (Arc::new(move |ev| e.lock().unwrap().push(ev)), eventos)
}

async fn propriedade<T>(conn: &zbus::Connection, nome_item: &str, prop: &str) -> T
where
    T: TryFrom<zbus::zvariant::OwnedValue>,
    T::Error: std::fmt::Debug,
{
    let p = zbus::fdo::PropertiesProxy::builder(conn)
        .destination(nome_item.to_string())
        .unwrap()
        .path(CAMINHO_ITEM)
        .unwrap()
        .build()
        .await
        .unwrap();
    let iface = zbus::names::InterfaceName::from_static_str("org.kde.StatusNotifierItem").unwrap();
    T::try_from(p.get(iface, prop).await.unwrap()).unwrap()
}

#[tokio::test]
async fn item_registra_responde_cliques_e_atualiza_rotulo() {
    let bus = BarramentoPrivado::novo();
    let conn_watcher = bus.conectar().await;
    let watcher = subir_watcher(&conn_watcher).await;

    let conn = bus.conectar().await;
    let (avisar, eventos) = coletor();
    let nome = publicar(&conn, ItemBandeja::novo(Icones::embutidos(), avisar))
        .await
        .unwrap();
    tokio::spawn(manter_registrado(conn.clone(), nome.clone()));
    assert!(
        esperar(|| watcher.0.lock().unwrap().contains(&nome)).await,
        "não se registrou no watcher"
    );

    // estado inicial: serviço ainda não consultado
    assert_eq!(
        propriedade::<String>(&conn_watcher, &nome, "XAyatanaLabel").await,
        "– W"
    );
    assert!(!propriedade::<bool>(&conn_watcher, &nome, "ItemIsMenu").await);

    // clique no ícone vira evento para a janela
    conn_watcher
        .call_method(
            Some(nome.as_str()),
            CAMINHO_ITEM,
            Some("org.kde.StatusNotifierItem"),
            "Activate",
            &(120i32, 15i32),
        )
        .await
        .unwrap();
    assert!(
        esperar(|| eventos
            .lock()
            .unwrap()
            .contains(&Evento::Alternar { x: 120, y: 15 }))
        .await
    );

    // atualizar troca o rótulo e a dica
    let a = Aparencia {
        rotulo: "142 W".into(),
        ativo: true,
        dica_titulo: "Kwanto".into(),
        dica_texto: "x".into(),
    };
    atualizar(&conn, a).await.unwrap();
    assert_eq!(
        propriedade::<String>(&conn_watcher, &nome, "XAyatanaLabel").await,
        "142 W"
    );
}

#[tokio::test]
async fn sem_bandeja_no_inicio_registra_quando_ela_aparece() {
    let bus = BarramentoPrivado::novo();
    let conn = bus.conectar().await;
    let (avisar, _) = coletor();
    let nome = publicar(&conn, ItemBandeja::novo(Icones::embutidos(), avisar))
        .await
        .unwrap();
    tokio::spawn(manter_registrado(conn.clone(), nome.clone()));
    tokio::time::sleep(std::time::Duration::from_millis(300)).await; // não há watcher: não pode cair

    let conn_watcher = bus.conectar().await;
    let watcher = subir_watcher(&conn_watcher).await;
    assert!(
        esperar(|| watcher.0.lock().unwrap().contains(&nome)).await,
        "não se registrou quando a bandeja apareceu"
    );
}

#[tokio::test]
async fn segunda_instancia_mostra_o_popup_da_primeira_e_desiste() {
    use kwanto::instancia::tentar_ser_unica;
    let bus = BarramentoPrivado::novo();
    let (avisar1, eventos1) = coletor();
    let (avisar2, _) = coletor();
    let conn1 = bus.conectar().await;
    let conn2 = bus.conectar().await;
    assert!(
        tentar_ser_unica(&conn1, avisar1).await.unwrap(),
        "a primeira deveria ficar"
    );
    assert!(
        !tentar_ser_unica(&conn2, avisar2).await.unwrap(),
        "a segunda deveria desistir"
    );
    assert!(
        esperar(|| eventos1.lock().unwrap().contains(&Evento::Mostrar)).await,
        "a primeira não mostrou o popup"
    );
}

/// Título e ações de cada notificação recebida.
type Pedidos = Arc<Mutex<Vec<(String, Vec<String>)>>>;

/// Imita o serviço de notificações: guarda o pedido e responde com o id 7.
#[derive(Clone, Default)]
struct NotificacoesFalsas(Pedidos);

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotificacoesFalsas {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _app: &str,
        _substitui: u32,
        _icone: &str,
        titulo: &str,
        _corpo: &str,
        acoes: Vec<String>,
        _dicas: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        _tempo: i32,
    ) -> u32 {
        self.0.lock().unwrap().push((titulo.to_string(), acoes));
        7
    }

    #[zbus(signal)]
    async fn action_invoked(
        e: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        acao: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn notification_closed(
        e: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        motivo: u32,
    ) -> zbus::Result<()>;
}

async fn servidor_de_notificacoes(
    bus: &BarramentoPrivado,
) -> (zbus::Connection, NotificacoesFalsas) {
    let conn = bus.conectar().await;
    let falsas = NotificacoesFalsas::default();
    conn.object_server()
        .at("/org/freedesktop/Notifications", falsas.clone())
        .await
        .unwrap();
    conn.request_name("org.freedesktop.Notifications")
        .await
        .unwrap();
    (conn, falsas)
}

async fn emissor(conn: &zbus::Connection) -> zbus::object_server::InterfaceRef<NotificacoesFalsas> {
    conn.object_server()
        .interface::<_, NotificacoesFalsas>("/org/freedesktop/Notifications")
        .await
        .unwrap()
}

#[tokio::test]
async fn oferta_de_atualizacao_e_resposta_do_usuario() {
    use kwanto::notificacao::{Notificador, Resposta};
    let bus = BarramentoPrivado::novo();
    let (conn_srv, falsas) = servidor_de_notificacoes(&bus).await;

    let conn = bus.conectar().await;
    let n = Notificador::novo(&conn).await.unwrap();
    let id = n
        .oferecer(&kwanto_atualizacao::versao::Versao::ler("0.2.0").unwrap())
        .await
        .unwrap();
    assert_eq!(id, 7);
    let (titulo, acoes) = falsas.0.lock().unwrap()[0].clone();
    assert!(titulo.contains("0.2.0"));
    assert_eq!(
        acoes,
        vec![
            "atualizar",
            "Atualizar",
            "agora-nao",
            "Agora não",
            "nunca",
            "Não perguntar mais"
        ]
    );

    for (acao, esperada) in [
        ("atualizar", Resposta::Atualizar),
        ("agora-nao", Resposta::AgoraNao),
        ("nunca", Resposta::NaoPerguntarMais),
    ] {
        let n2 = n.clone();
        let resposta = tokio::spawn(async move { n2.esperar_resposta(7).await });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        let iface = emissor(&conn_srv).await;
        // ação de outra notificação é ignorada
        NotificacoesFalsas::action_invoked(iface.signal_emitter(), 99, "atualizar")
            .await
            .unwrap();
        NotificacoesFalsas::action_invoked(iface.signal_emitter(), 7, acao)
            .await
            .unwrap();
        assert_eq!(resposta.await.unwrap(), esperada);
    }

    let n2 = n.clone();
    let resposta = tokio::spawn(async move { n2.esperar_resposta(7).await });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    NotificacoesFalsas::notification_closed(emissor(&conn_srv).await.signal_emitter(), 7, 2)
        .await
        .unwrap();
    assert_eq!(resposta.await.unwrap(), Resposta::Fechou);
}
