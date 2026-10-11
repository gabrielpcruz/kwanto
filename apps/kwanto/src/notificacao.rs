//! Notificações do sistema (padrão freedesktop): oferta de atualização com botões e avisos.
use futures_util::StreamExt as _;
use kwanto_atualizacao::versao::Versao;
use std::collections::HashMap;
use zbus::zvariant::Value;

#[zbus::proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notificacoes {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app: &str,
        substitui: u32,
        icone: &str,
        titulo: &str,
        corpo: &str,
        acoes: &[&str],
        dicas: HashMap<&str, Value<'_>>,
        tempo: i32,
    ) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn action_invoked(&self, id: u32, acao: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn notification_closed(&self, id: u32, motivo: u32) -> zbus::Result<()>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resposta {
    Atualizar,
    AgoraNao,
    NaoPerguntarMais,
    Fechou,
}

#[derive(Debug, Clone)]
pub struct Notificador(NotificacoesProxy<'static>);

impl Notificador {
    pub async fn novo(conn: &zbus::Connection) -> zbus::Result<Self> {
        Ok(Self(NotificacoesProxy::new(conn).await?))
    }

    /// Notificação persistente com "Atualizar", "Agora não" e "Não perguntar mais".
    pub async fn oferecer(&self, versao: &Versao) -> zbus::Result<u32> {
        let acoes = [
            "atualizar",
            "Atualizar",
            "agora-nao",
            "Agora não",
            "nunca",
            "Não perguntar mais",
        ];
        let mut dicas = HashMap::new();
        dicas.insert("resident", Value::from(true));
        self.0
            .notify(
                "Kwanto",
                0,
                "kwanto",
                &format!("Kwanto {versao} disponível"),
                "Há uma versão nova do Kwanto. Para instalar, o sistema vai pedir a senha de administrador.",
                &acoes,
                dicas,
                0,
            )
            .await
    }

    pub async fn avisar(&self, titulo: &str, texto: &str) -> zbus::Result<()> {
        self.0
            .notify(
                "Kwanto",
                0,
                "kwanto",
                titulo,
                texto,
                &[],
                HashMap::new(),
                8000,
            )
            .await
            .map(|_| ())
    }

    /// Espera o usuário escolher uma ação (ou fechar) a notificação `id`.
    pub async fn esperar_resposta(&self, id: u32) -> Resposta {
        let (Ok(mut acoes), Ok(mut fechadas)) = (
            self.0.receive_action_invoked().await,
            self.0.receive_notification_closed().await,
        ) else {
            return Resposta::Fechou;
        };
        loop {
            tokio::select! {
                Some(s) = acoes.next() => {
                    let escolha = s.args().ok().filter(|a| a.id == id).map(|a| match a.acao.as_str() {
                        "atualizar" => Resposta::Atualizar,
                        "nunca" => Resposta::NaoPerguntarMais,
                        _ => Resposta::AgoraNao,
                    });
                    if let Some(r) = escolha {
                        return r;
                    }
                }
                Some(s) = fechadas.next() => {
                    if s.args().is_ok_and(|a| a.id == id) {
                        return Resposta::Fechou;
                    }
                }
                else => return Resposta::Fechou,
            }
        }
    }
}
