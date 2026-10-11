//! Consulta o serviço a cada poucos segundos, atualiza o ícone e avisa a janela quando o serviço
//! cai ou volta. Antes de usar qualquer resposta, confere no kernel que o socket do outro lado
//! pertence ao usuário do serviço: outro usuário que ocupasse a porta (com o serviço parado) não
//! consegue mostrar nada na bandeja nem no popup.
use crate::aparencia::Aparencia;
use crate::{Avisar, Evento};
use async_trait::async_trait;
use serde_json::Value;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const TEMPO_LIMITE: Duration = Duration::from_secs(3);
/// Maior resposta aceita do serviço.
const LIMITE_RESPOSTA: u64 = 4 * 1024 * 1024;

/// Onde o resumo aparece. Linux: StatusNotifierItem; Windows (futuro): tray-icon.
#[async_trait]
pub trait Bandeja: Send + Sync {
    async fn mostrar(&self, aparencia: Aparencia);
}

/// Último status verificado, em JSON, para o popup.
pub type UltimoStatus = Arc<Mutex<Option<String>>>;

/// O serviço local e quem pode ser dono dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Servico {
    pub endereco: SocketAddr,
    pub donos: Vec<u32>,
}

impl Servico {
    /// Só `http://127.0.0.1:<porta>` ou `http://localhost:<porta>`.
    #[must_use]
    pub fn da_url(url: &str, donos: Vec<u32>) -> Option<Self> {
        let resto = url.strip_prefix("http://")?.trim_end_matches('/');
        let (host, porta) = resto.rsplit_once(':')?;
        let porta: u16 = porta.parse().ok()?;
        (host == "127.0.0.1" || host == "localhost").then(|| Self {
            endereco: SocketAddr::from(([127, 0, 0, 1], porta)),
            donos,
        })
    }
}

/// UID de um usuário num texto no formato de /etc/passwd.
#[must_use]
pub fn uid_no_passwd(passwd: &str, usuario: &str) -> Option<u32> {
    passwd.lines().find_map(|l| {
        let mut campos = l.split(':');
        (campos.next()? == usuario).then(|| campos.nth(1)?.parse().ok())?
    })
}

/// Donos aceitos: o usuário de sistema `kwanto` (pacote). Com `KWANTO_URL` definida pelo próprio
/// usuário (desenvolvimento), também ele mesmo.
#[must_use]
pub fn donos_confiaveis(url_explicita: bool) -> Vec<u32> {
    let mut donos: Vec<u32> = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|p| uid_no_passwd(&p, "kwanto"))
        .into_iter()
        .collect();
    if url_explicita {
        donos.push(rustix::process::geteuid().as_raw());
    }
    donos
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resposta {
    NoAr(Value),
    Fora,
    /// Alguém responde na porta, mas não é o serviço (UID do dono).
    DonoEstranho(u32),
}

/// GET mínimo em HTTP/1.1 sobre uma conexão cujo dono do outro lado foi conferido.
fn get_verificado(s: &Servico, caminho: &str) -> Result<Vec<u8>, Resposta> {
    let mut conexao =
        TcpStream::connect_timeout(&s.endereco, TEMPO_LIMITE).map_err(|_| Resposta::Fora)?;
    conexao
        .set_read_timeout(Some(TEMPO_LIMITE))
        .map_err(|_| Resposta::Fora)?;
    conexao
        .set_write_timeout(Some(TEMPO_LIMITE))
        .map_err(|_| Resposta::Fora)?;
    let local = conexao.local_addr().map_err(|_| Resposta::Fora)?;
    // O socket do servidor nesta conexão: endereço local = o do serviço, remoto = o nosso.
    let dono = kwanto_core::soquetes::dono(s.endereco, local).ok_or(Resposta::Fora)?;
    if !s.donos.contains(&dono) {
        return Err(Resposta::DonoEstranho(dono));
    }
    // Numa escrita só: o pedido inteiro chega junto ao servidor.
    let pedido = format!(
        "GET {caminho} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n\r\n",
        s.endereco
    );
    conexao
        .write_all(pedido.as_bytes())
        .map_err(|_| Resposta::Fora)?;
    let mut bruto = Vec::new();
    conexao
        .take(LIMITE_RESPOSTA + 1)
        .read_to_end(&mut bruto)
        .map_err(|_| Resposta::Fora)?;
    corpo_200(&bruto).ok_or(Resposta::Fora)
}

/// Corpo de uma resposta 200 (com `Content-Length` ou `chunked`); `None` para qualquer outra.
fn corpo_200(bruto: &[u8]) -> Option<Vec<u8>> {
    let fim = bruto.windows(4).position(|j| j == b"\r\n\r\n")?;
    let cabecalho = std::str::from_utf8(&bruto[..fim])
        .ok()?
        .to_ascii_lowercase();
    let corpo = &bruto[fim + 4..];
    if !cabecalho.lines().next()?.starts_with("http/1.1 200") {
        return None;
    }
    if cabecalho
        .lines()
        .any(|l| l.replace(' ', "") == "transfer-encoding:chunked")
    {
        let mut saida = Vec::new();
        let mut resto = corpo;
        loop {
            let fim_tam = resto.windows(2).position(|j| j == b"\r\n")?;
            let tam = usize::from_str_radix(
                std::str::from_utf8(&resto[..fim_tam])
                    .ok()?
                    .split(';')
                    .next()?
                    .trim(),
                16,
            )
            .ok()?;
            if tam == 0 {
                return Some(saida);
            }
            let inicio = fim_tam + 2;
            saida.extend_from_slice(resto.get(inicio..inicio + tam)?);
            resto = resto.get(inicio + tam + 2..)?;
        }
    }
    let tamanho = cabecalho.lines().find_map(|l| {
        l.strip_prefix("content-length:")
            .and_then(|v| v.trim().parse::<usize>().ok())
    });
    Some(match tamanho {
        Some(t) => corpo.get(..t)?.to_vec(),
        None => corpo.to_vec(),
    })
}

#[must_use]
pub fn buscar_status(s: &Servico) -> Resposta {
    match get_verificado(s, "/api/status") {
        Ok(corpo) => serde_json::from_slice::<Value>(&corpo)
            .ok()
            .filter(Value::is_object)
            .map_or(Resposta::Fora, Resposta::NoAr),
        Err(r) => r,
    }
}

/// Para sempre: atualiza a bandeja a cada `intervalo`, guarda o status para o popup e avisa a
/// janela só quando o serviço muda de estado. `None`: endereço inválido, nunca confia.
pub async fn acompanhar(
    servico: Option<Servico>,
    ultimo: UltimoStatus,
    bandeja: Arc<dyn Bandeja>,
    avisar: Avisar,
    intervalo: Duration,
) {
    let mut anterior: Option<bool> = None;
    let mut estranho_avisado = None;
    loop {
        let resposta = match servico.clone() {
            Some(s) => tokio::task::spawn_blocking(move || buscar_status(&s))
                .await
                .unwrap_or(Resposta::Fora),
            None => Resposta::Fora,
        };
        let status = match resposta {
            Resposta::NoAr(v) => {
                estranho_avisado = None;
                Some(v)
            }
            Resposta::DonoEstranho(uid) => {
                if estranho_avisado != Some(uid) {
                    tracing::warn!(
                        uid,
                        "AVISO DE SEGURANÇA: outro programa responde na porta do Kwanto; ignorado"
                    );
                    estranho_avisado = Some(uid);
                }
                None
            }
            Resposta::Fora => None,
        };
        if let Ok(mut u) = ultimo.lock() {
            *u = status.as_ref().map(Value::to_string);
        }
        let no_ar = status.is_some();
        bandeja.mostrar(Aparencia::de_status(status.as_ref())).await;
        if anterior != Some(no_ar) {
            avisar(Evento::Servico(no_ar));
            anterior = Some(no_ar);
        }
        tokio::time::sleep(intervalo).await;
    }
}

#[cfg(test)]
mod testes {
    use super::corpo_200;

    #[test]
    fn respostas_http() {
        assert_eq!(
            corpo_200(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}lixo").unwrap(),
            b"{}"
        );
        assert_eq!(
            corpo_200(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n{\"\r\n4\r\na\":1\r\n1\r\n}\r\n0\r\n\r\n").unwrap(),
            b"{\"a\":1}"
        );
        assert!(corpo_200(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 2\r\n\r\n{}").is_none());
        assert!(corpo_200(b"lixo").is_none());
    }
}
