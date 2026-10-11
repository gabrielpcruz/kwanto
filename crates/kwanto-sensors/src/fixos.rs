//! Componentes sem sensor com valor fixo ajustável pelo usuário.
use kwanto_core::componente::{Amostra, Componente, ComponenteId, Perfil};
use kwanto_core::sensores::Medidor;
use std::time::Duration;

#[derive(Debug, Default)]
pub struct ValoresFixos;

impl Medidor for ValoresFixos {
    fn componentes(&self) -> Vec<Componente> {
        vec![
            Componente::novo("ram", "RAM", "valor fixo", Perfil::Ram),
            Componente::novo("placa_mae", "Placa-mãe", "valor fixo", Perfil::PlacaMae),
            Componente::novo("fans_usb", "Fans e USB", "valor fixo", Perfil::FansUsb),
        ]
    }

    fn ler(&mut self, _dt: Duration) -> Vec<(ComponenteId, Amostra)> {
        self.componentes()
            .into_iter()
            .map(|c| (c.id, Amostra::Fixo))
            .collect()
    }
}
