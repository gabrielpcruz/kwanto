//! Bandeja do Kwanto: ícone com o consumo na barra do sistema e popup com o resumo.
pub mod aparencia;
pub mod atualizador;
pub mod conteudo;
pub mod estado_popup;
pub mod icones;
pub mod instancia;
pub mod monitor;
pub mod notificacao;
pub mod ponte;
pub mod popup;
pub mod posicao;
pub mod preferencias;
pub mod sni;

use std::sync::Arc;

/// Eventos entregues ao loop da janela (thread principal).
#[derive(Debug, Clone, PartialEq)]
pub enum Evento {
    /// Clique no ícone, com as coordenadas de tela informadas pela bandeja (0,0 se desconhecidas).
    Alternar { x: i32, y: i32 },
    /// Pedido para mostrar o popup (app aberto de novo).
    Mostrar,
    /// O serviço passou a responder (true) ou deixou de responder (false).
    Servico(bool),
    /// Mensagem vinda da página do popup.
    Popup(ponte::MensagemPopup),
    /// A atualização foi instalada: trocar este processo pelo binário novo.
    ReiniciarBandeja,
}

/// Como os módulos assíncronos avisam o loop da janela.
pub type Avisar = Arc<dyn Fn(Evento) + Send + Sync>;
