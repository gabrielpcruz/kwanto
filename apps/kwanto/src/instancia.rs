//! Uma bandeja só: abrir o app de novo mostra o popup da que já está rodando.
use crate::{Avisar, Evento};
use zbus::fdo::{RequestNameFlags, RequestNameReply};

pub const NOME_APP: &str = "io.github.gabrielpcruz.Kwanto";
pub const CAMINHO_APP: &str = "/io/github/gabrielpcruz/Kwanto";

struct Aplicativo(Avisar);

#[zbus::interface(name = "io.github.gabrielpcruz.Kwanto")]
impl Aplicativo {
    fn mostrar(&self) {
        (self.0)(Evento::Mostrar);
    }
}

/// `true` se esta é a primeira instância; `false` se já havia outra (que recebeu `Mostrar`).
pub async fn tentar_ser_unica(conn: &zbus::Connection, avisar: Avisar) -> zbus::Result<bool> {
    conn.object_server()
        .at(CAMINHO_APP, Aplicativo(avisar))
        .await?;
    let ocupado = match conn
        .request_name_with_flags(NOME_APP, RequestNameFlags::DoNotQueue.into())
        .await
    {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => false,
        Ok(_) | Err(zbus::Error::NameTaken) => true,
        Err(e) => return Err(e),
    };
    if !ocupado {
        return Ok(true);
    }
    conn.object_server()
        .remove::<Aplicativo, _>(CAMINHO_APP)
        .await?;
    conn.call_method(Some(NOME_APP), CAMINHO_APP, Some(NOME_APP), "Mostrar", &())
        .await?;
    Ok(false)
}
