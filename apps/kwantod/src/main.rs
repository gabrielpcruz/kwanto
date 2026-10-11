//! Binário do serviço na versão aberta: sem extensões.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .without_time() // o journal já carimba a hora
        .init();
    kwantod::executar(
        kwantod::plataforma_linux()?,
        kwantod::Extensoes::default(),
        kwantod::sinal_de_desligar(),
    )
    .await
}
