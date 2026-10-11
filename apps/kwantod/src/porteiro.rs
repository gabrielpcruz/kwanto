//! Porteiro real da API: descobre o usuário dono da conexão local pela tabela de sockets do kernel
//! e confere no logind se ele tem a sessão ativa na máquina.

use kwanto_api::acesso::{Acesso, Porteiro};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Por quanto tempo a lista de sessões ativas do logind vale.
const VALIDADE_SESSOES: Duration = Duration::from_secs(5);

fn uid_do_cliente(servidor: SocketAddr, cliente: SocketAddr) -> Option<u32> {
    kwanto_core::soquetes::dono(cliente, servidor)
}

/// UIDs com sessão ativa, local e num seat (a pessoa diante da máquina).
#[async_trait::async_trait]
pub trait Sessoes: Send + Sync + std::fmt::Debug {
    async fn uids_ativos(&self) -> Result<Vec<u32>, String>;
}

/// Consulta o logind pelo D-Bus do sistema.
#[derive(Debug)]
pub struct Logind(Option<zbus::Connection>);

impl Logind {
    pub async fn conectar() -> Self {
        match zbus::Connection::system().await {
            Ok(c) => Self(Some(c)),
            Err(e) => {
                tracing::warn!(erro = %e, "sem acesso ao logind: a API só vai responder ao root");
                Self(None)
            }
        }
    }
}

#[async_trait::async_trait]
impl Sessoes for Logind {
    async fn uids_ativos(&self) -> Result<Vec<u32>, String> {
        let conn = self
            .0
            .as_ref()
            .ok_or("sem conexão com o D-Bus do sistema")?;
        let erro = |e: zbus::Error| e.to_string();
        let gerente = zbus::Proxy::new(
            conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .await
        .map_err(erro)?;
        let sessoes: Vec<(String, u32, String, String, zbus::zvariant::OwnedObjectPath)> =
            gerente.call("ListSessions", &()).await.map_err(erro)?;
        let mut uids = Vec::new();
        for (_, uid, _, seat, caminho) in sessoes {
            if seat.is_empty() {
                continue;
            }
            let sessao = zbus::Proxy::new(
                conn,
                "org.freedesktop.login1",
                caminho,
                "org.freedesktop.login1.Session",
            )
            .await
            .map_err(erro)?;
            let ativa: bool = sessao.get_property("Active").await.map_err(erro)?;
            let remota: bool = sessao.get_property("Remote").await.map_err(erro)?;
            if ativa && !remota {
                uids.push(uid);
            }
        }
        Ok(uids)
    }
}

type Identificar = Box<dyn Fn(SocketAddr) -> Option<u32> + Send + Sync>;

/// Permite: root, o próprio usuário do serviço (dono do banco) e quem tem a sessão ativa.
pub struct PorteiroSistema {
    sessoes: Arc<dyn Sessoes>,
    identificar: Identificar,
    proprio_uid: u32,
    cache: Mutex<Option<(Instant, Vec<u32>)>>,
}

impl std::fmt::Debug for PorteiroSistema {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PorteiroSistema")
            .field("proprio_uid", &self.proprio_uid)
            .finish_non_exhaustive()
    }
}

impl PorteiroSistema {
    pub fn novo(servidor: SocketAddr, sessoes: Arc<dyn Sessoes>) -> Self {
        Self::novo_com(servidor, sessoes, move |cliente| {
            uid_do_cliente(servidor, cliente)
        })
    }

    fn novo_com(
        _servidor: SocketAddr,
        sessoes: Arc<dyn Sessoes>,
        identificar: impl Fn(SocketAddr) -> Option<u32> + Send + Sync + 'static,
    ) -> Self {
        Self {
            sessoes,
            identificar: Box::new(identificar),
            proprio_uid: rustix::process::geteuid().as_raw(),
            cache: Mutex::new(None),
        }
    }

    async fn ativos(&self) -> Result<Vec<u32>, String> {
        if let Some((quando, uids)) = self.cache.lock().ok().and_then(|c| c.clone()) {
            if quando.elapsed() < VALIDADE_SESSOES {
                return Ok(uids);
            }
        }
        let uids = self.sessoes.uids_ativos().await?;
        if let Ok(mut c) = self.cache.lock() {
            *c = Some((Instant::now(), uids.clone()));
        }
        Ok(uids)
    }
}

#[async_trait::async_trait]
impl Porteiro for PorteiroSistema {
    async fn decidir(&self, cliente: SocketAddr) -> Acesso {
        let Some(uid) = (self.identificar)(cliente) else {
            return Acesso::Negado("não foi possível identificar o dono da conexão");
        };
        if uid == 0 || uid == self.proprio_uid {
            return Acesso::Permitido;
        }
        match self.ativos().await {
            Ok(uids) if uids.contains(&uid) => Acesso::Permitido,
            Ok(_) => Acesso::Negado("usuário sem sessão ativa na máquina"),
            Err(_) => Acesso::Negado("logind indisponível"),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::os::unix::fs::MetadataExt as _;

    fn end(porta: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], porta))
    }

    /// Ataque: pedir e fechar a conexão na hora. O socket órfão aparece como do root no kernel;
    /// o porteiro não pode tratar isso como root.
    #[tokio::test]
    async fn conexao_fechada_pelo_cliente_e_negada() {
        let ouvinte = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let servidor = ouvinte.local_addr().unwrap();
        let cliente = tokio::net::TcpStream::connect(servidor).await.unwrap();
        let (_aceita, endereco_cliente) = ouvinte.accept().await.unwrap();
        let porteiro = PorteiroSistema::novo(servidor, Arc::new(SessoesFalsas(Ok(vec![]))));
        assert_eq!(
            porteiro.decidir(endereco_cliente).await,
            Acesso::Permitido,
            "viva: é o próprio usuário"
        );
        drop(cliente);
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(
            matches!(porteiro.decidir(endereco_cliente).await, Acesso::Negado(_)),
            "fechada: ninguém"
        );
    }

    #[tokio::test]
    async fn conexao_real_e_atribuida_ao_proprio_usuario() {
        let ouvinte = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let servidor = ouvinte.local_addr().unwrap();
        let _cliente = tokio::net::TcpStream::connect(servidor).await.unwrap();
        let (_s, cliente) = ouvinte.accept().await.unwrap();
        let meu_uid = std::fs::metadata("/proc/self").unwrap().uid();
        assert_eq!(uid_do_cliente(servidor, cliente), Some(meu_uid));
    }

    #[derive(Debug)]
    struct SessoesFalsas(Result<Vec<u32>, ()>);

    #[async_trait::async_trait]
    impl Sessoes for SessoesFalsas {
        async fn uids_ativos(&self) -> Result<Vec<u32>, String> {
            self.0.clone().map_err(|()| "logind fora do ar".into())
        }
    }

    #[tokio::test]
    async fn decide_por_root_sessao_ativa_e_falha_fechada() {
        let proprio = rustix::process::geteuid().as_raw();
        let p = |s| {
            PorteiroSistema::novo_com(
                end(8787),
                Arc::new(SessoesFalsas(s)),
                move |c: SocketAddr| match c.port() {
                    1 => Some(0),
                    2 => Some(4001),
                    3 => Some(4002),
                    5 => Some(proprio),
                    _ => None,
                },
            )
        };
        let ativo = p(Ok(vec![4001]));
        assert_eq!(ativo.decidir(end(1)).await, Acesso::Permitido, "root");
        assert_eq!(
            ativo.decidir(end(2)).await,
            Acesso::Permitido,
            "sessão ativa"
        );
        assert_eq!(
            ativo.decidir(end(5)).await,
            Acesso::Permitido,
            "usuário do próprio serviço"
        );
        assert!(
            matches!(ativo.decidir(end(3)).await, Acesso::Negado(_)),
            "outro usuário"
        );
        assert!(
            matches!(ativo.decidir(end(4)).await, Acesso::Negado(_)),
            "dono desconhecido"
        );
        let sem_logind = p(Err(()));
        assert!(
            matches!(sem_logind.decidir(end(2)).await, Acesso::Negado(_)),
            "logind fora do ar"
        );
        assert_eq!(
            sem_logind.decidir(end(1)).await,
            Acesso::Permitido,
            "root não depende do logind"
        );
    }

    /// Exige uma sessão gráfica ativa (não roda no CI): `cargo test -p kwantod -- --ignored`.
    #[tokio::test]
    #[ignore = "precisa de logind com uma sessão ativa"]
    async fn logind_real_lista_a_sessao_deste_usuario() {
        let meu_uid = std::fs::metadata("/proc/self").unwrap().uid();
        let uids = Logind::conectar().await.uids_ativos().await.unwrap();
        assert!(uids.contains(&meu_uid), "{uids:?} sem {meu_uid}");
    }
}
