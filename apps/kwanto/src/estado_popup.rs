//! Regras do popup sem interface gráfica (testáveis): quando abrir, fechar, reposicionar e
//! recarregar a página.
use std::time::{Duration, Instant};

/// Perder o foco logo após abrir é o próprio clique no ícone: ignorar.
pub const CARENCIA_FOCO: Duration = Duration::from_millis(400);
/// Um clique no ícone até este tempo após o popup fechar por perda de foco é o mesmo clique.
pub const MESMO_CLIQUE: Duration = Duration::from_millis(300);
pub const ALTURA_INICIAL: u32 = 480;
/// Tentativas de recarregar uma página que não dá sinal de vida (a cada 3 s: 30 s no total).
pub const MAX_RECARGAS: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Mostrar,
    Esconder,
    Nada,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pagina {
    /// O /mini do serviço.
    Mini,
    /// A página local que explica como iniciar o serviço.
    Parado,
}

#[derive(Debug)]
pub struct EstadoPopup {
    visivel: bool,
    mostrado_em: Option<Instant>,
    escondido_por_foco_em: Option<Instant>,
    altura: u32,
    clique: Option<(i32, i32)>,
    servico_ok: bool,
    pagina_ok: bool,
    recargas: u32,
}

impl Default for EstadoPopup {
    fn default() -> Self {
        Self::novo()
    }
}

impl EstadoPopup {
    #[must_use]
    pub fn novo() -> Self {
        Self {
            visivel: false,
            mostrado_em: None,
            escondido_por_foco_em: None,
            altura: ALTURA_INICIAL,
            clique: None,
            servico_ok: true,
            pagina_ok: false,
            recargas: 0,
        }
    }

    #[must_use]
    pub fn visivel(&self) -> bool {
        self.visivel
    }

    #[must_use]
    pub fn altura(&self) -> u32 {
        self.altura
    }

    /// Último ponto de clique usado para posicionar.
    #[must_use]
    pub fn clique(&self) -> Option<(i32, i32)> {
        self.clique
    }

    pub fn mostrar(&mut self, clique: Option<(i32, i32)>, agora: Instant) -> Acao {
        self.visivel = true;
        self.clique = clique;
        self.mostrado_em = Some(agora);
        self.escondido_por_foco_em = None;
        Acao::Mostrar
    }

    pub fn esconder(&mut self) -> Acao {
        if self.visivel {
            self.visivel = false;
            Acao::Esconder
        } else {
            Acao::Nada
        }
    }

    pub fn alternar(&mut self, clique: Option<(i32, i32)>, agora: Instant) -> Acao {
        if self.visivel {
            return self.esconder();
        }
        let mesmo_clique = self
            .escondido_por_foco_em
            .is_some_and(|t| agora.saturating_duration_since(t) < MESMO_CLIQUE);
        if mesmo_clique {
            self.escondido_por_foco_em = None;
            return Acao::Nada;
        }
        self.mostrar(clique, agora)
    }

    pub fn perdeu_foco(&mut self, agora: Instant) -> Acao {
        let recem_aberto = self
            .mostrado_em
            .is_some_and(|t| agora.saturating_duration_since(t) < CARENCIA_FOCO);
        if !self.visivel || recem_aberto {
            return Acao::Nada;
        }
        self.escondido_por_foco_em = Some(agora);
        self.esconder()
    }

    /// Guarda a altura pedida pela página; devolve `true` se o popup visível precisa ser reposicionado.
    pub fn nova_altura(&mut self, altura: u32) -> bool {
        self.altura = altura;
        self.visivel
    }

    /// O serviço caiu ou voltou: qual página carregar (ou `None` se nada mudou).
    pub fn servico(&mut self, ok: bool) -> Option<Pagina> {
        if ok == self.servico_ok {
            return None;
        }
        self.servico_ok = ok;
        if ok {
            self.pagina_ok = false;
            self.recargas = 0;
            Some(Pagina::Mini)
        } else {
            Some(Pagina::Parado)
        }
    }

    /// A página mandou uma mensagem: carregou e está viva.
    pub fn mensagem_recebida(&mut self) {
        self.pagina_ok = true;
        self.recargas = 0;
    }

    /// Registra uma tentativa de recarregar (para não insistir para sempre numa página que não fala).
    pub fn recarregou(&mut self) {
        self.recargas += 1;
    }

    /// O serviço responde, mas a página ainda não deu sinal de vida: tentar carregar de novo.
    #[must_use]
    pub fn precisa_recarregar(&self) -> bool {
        self.servico_ok && !self.pagina_ok && self.recargas < MAX_RECARGAS
    }
}

/// Tamanho da janela em pixels físicos, a partir da largura e altura lógicas.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn tamanho_fisico(largura: f64, altura: u32, escala: f64) -> (u32, u32) {
    (
        (largura * escala).round() as u32,
        (f64::from(altura) * escala).round() as u32,
    )
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::time::{Duration, Instant};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn clique_no_icone_abre_e_fecha() {
        let mut e = EstadoPopup::novo();
        let t = Instant::now();
        assert_eq!(e.alternar(Some((10, 15)), t), Acao::Mostrar);
        assert_eq!(e.clique(), Some((10, 15)));
        assert_eq!(e.alternar(Some((10, 15)), t + ms(1000)), Acao::Esconder);
        assert!(!e.visivel());
    }

    #[test]
    fn clique_no_icone_que_tirou_o_foco_nao_reabre() {
        // Em algumas bandejas o clique no ícone tira o foco do popup antes de chegar o Activate.
        let mut e = EstadoPopup::novo();
        let t = Instant::now();
        e.alternar(None, t);
        assert_eq!(e.perdeu_foco(t + ms(1000)), Acao::Esconder);
        assert_eq!(
            e.alternar(None, t + ms(1100)),
            Acao::Nada,
            "o mesmo clique não pode reabrir"
        );
        assert_eq!(
            e.alternar(None, t + ms(2000)),
            Acao::Mostrar,
            "um clique novo depois reabre"
        );
    }

    #[test]
    fn perda_de_foco_logo_apos_abrir_e_ignorada() {
        let mut e = EstadoPopup::novo();
        let t = Instant::now();
        e.alternar(None, t);
        assert_eq!(e.perdeu_foco(t + ms(100)), Acao::Nada);
        assert!(e.visivel());
    }

    #[test]
    fn nova_altura_pede_reposicionar_so_com_popup_visivel() {
        let mut e = EstadoPopup::novo();
        assert!(!e.nova_altura(300));
        assert_eq!(e.altura(), 300);
        e.alternar(None, Instant::now());
        assert!(e.nova_altura(410));
        assert_eq!(e.altura(), 410);
    }

    #[test]
    fn troca_de_pagina_so_quando_o_servico_muda() {
        let mut e = EstadoPopup::novo();
        assert_eq!(e.servico(true), None);
        assert_eq!(e.servico(false), Some(Pagina::Parado));
        assert_eq!(e.servico(false), None);
        assert_eq!(e.servico(true), Some(Pagina::Mini));
    }

    #[test]
    fn pagina_que_nao_carregou_e_recarregada() {
        let mut e = EstadoPopup::novo();
        assert!(
            e.precisa_recarregar(),
            "nenhuma mensagem da página ainda: recarregar"
        );
        e.mensagem_recebida();
        assert!(!e.precisa_recarregar());
        e.servico(false);
        assert!(
            !e.precisa_recarregar(),
            "serviço fora: a página local já está na tela"
        );
        e.servico(true);
        assert!(
            e.precisa_recarregar(),
            "acabou de pedir o /mini e ainda não respondeu"
        );
    }

    #[test]
    fn pagina_que_nunca_responde_desiste_depois_de_algumas_tentativas() {
        let mut e = EstadoPopup::novo();
        for _ in 0..MAX_RECARGAS {
            assert!(e.precisa_recarregar());
            e.recarregou();
        }
        assert!(!e.precisa_recarregar(), "não pode recarregar para sempre");
        e.servico(false);
        e.servico(true);
        assert!(e.precisa_recarregar(), "o serviço voltou: tenta de novo");
    }

    #[test]
    fn tamanho_em_pixels_fisicos() {
        assert_eq!(tamanho_fisico(360.0, 410, 1.0), (360, 410));
        assert_eq!(tamanho_fisico(360.0, 410, 2.0), (720, 820));
    }
}
