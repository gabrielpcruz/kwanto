//! Janela do popup: sem bordas, transparente, por cima de tudo, junto ao ícone.
//! As decisões (abrir, fechar, reposicionar, recarregar) ficam em `EstadoPopup`, que é testável.
use crate::conteudo;
use crate::estado_popup::{Acao, EstadoPopup, tamanho_fisico};
use crate::monitor::UltimoStatus;
use crate::ponte::interpretar;
use crate::posicao::{Retangulo, posicionar};
use crate::{Avisar, Evento};
use gtk::prelude::*;
use std::borrow::Cow;
use std::time::Instant;
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event_loop::EventLoopWindowTarget;
use tao::platform::unix::{WindowBuilderExtUnix, WindowExtUnix};
use tao::window::{Window, WindowBuilder};
use wry::http::{Response, header};
use wry::{NewWindowResponse, WebView, WebViewBuilder, WebViewBuilderExtUnix};

pub const LARGURA: f64 = 360.0;

/// O popup só carrega o próprio conteúdo embutido; o mesmo princípio do CSP do dashboard.
const CSP_POPUP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
    img-src 'self' data:; font-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; \
    form-action 'none'";

pub struct Popup {
    janela: Window,
    web: WebView,
    estado: EstadoPopup,
    /// O monitor conferiu que o serviço no ar é o do Kwanto.
    servico_ok: bool,
}

impl std::fmt::Debug for Popup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Popup")
            .field("estado", &self.estado)
            .finish_non_exhaustive()
    }
}

/// Área útil (sem barras e painéis) do monitor do clique, em pixels físicos.
fn area_util(clique: Option<(i32, i32)>) -> Option<Retangulo> {
    let tela = gtk::gdk::Display::default()?;
    let monitor = match clique {
        Some((x, y)) if (x, y) != (0, 0) => tela.monitor_at_point(x, y),
        _ => tela.primary_monitor().or_else(|| tela.monitor(0)),
    }?;
    let a = monitor.workarea();
    let e = monitor.scale_factor();
    Some(Retangulo {
        x: a.x() * e,
        y: a.y() * e,
        largura: u32::try_from(a.width() * e).ok()?,
        altura: u32::try_from(a.height() * e).ok()?,
    })
}

impl Popup {
    pub fn novo(
        alvo: &EventLoopWindowTarget<Evento>,
        ultimo: UltimoStatus,
        avisar: Avisar,
    ) -> anyhow::Result<Self> {
        let janela = WindowBuilder::new()
            .with_title("Kwanto")
            .with_decorations(false)
            // Redimensionável: o GTK ignora resize() em janela fixa, e a altura acompanha o conteúdo.
            // Sem decoração, o usuário não consegue redimensionar.
            .with_resizable(true)
            .with_always_on_top(true)
            .with_visible(false)
            .with_transparent(true)
            .with_skip_taskbar(true)
            .with_inner_size(LogicalSize::new(LARGURA, 480.0))
            .build(alvo)?;
        // Janela utilitária: o gerenciador não a trata como janela comum (posição automática, Alt+Tab).
        janela
            .gtk_window()
            .set_type_hint(gtk::gdk::WindowTypeHint::Utility);
        let caixa = janela
            .default_vbox()
            .ok_or_else(|| anyhow::anyhow!("janela GTK sem área de conteúdo"))?;
        let web = WebViewBuilder::new()
            .with_url(conteudo::PAGINA)
            .with_transparent(true)
            // Conteúdo embutido no binário; os dados vêm do monitor, já verificados.
            .with_custom_protocol("kwanto".into(), move |_id, req| {
                let r = conteudo::responder(req.uri().path(), &ultimo);
                Response::builder()
                    .status(r.status)
                    .header(header::CONTENT_TYPE, r.tipo)
                    .header("Content-Security-Policy", CSP_POPUP)
                    .body(r.corpo)
                    .unwrap_or_else(|_| Response::new(Cow::Borrowed(&[][..])))
            })
            // Nunca sai da própria origem nem abre outra janela.
            .with_navigation_handler(|url| {
                let ok = conteudo::da_propria_origem(&url);
                if !ok {
                    tracing::warn!(%url, "navegação do popup recusada");
                }
                ok
            })
            .with_new_window_req_handler(|url, _| {
                tracing::warn!(%url, "janela nova pedida pelo popup recusada");
                NewWindowResponse::Deny
            })
            .with_ipc_handler(move |req| {
                if !conteudo::da_propria_origem(&req.uri().to_string()) {
                    tracing::warn!(origem = %req.uri(), "mensagem de origem estranha ignorada");
                    return;
                }
                tracing::debug!(mensagem = %req.body(), "mensagem do popup");
                if let Some(m) = interpretar(req.body()) {
                    avisar(Evento::Popup(m));
                }
            })
            .build_gtk(caixa)?;
        Ok(Self {
            janela,
            web,
            estado: EstadoPopup::novo(),
            servico_ok: false,
        })
    }

    /// Só abre o painel no navegador se o serviço no ar foi verificado.
    #[must_use]
    pub fn servico_ok(&self) -> bool {
        self.servico_ok
    }

    fn js(&self, codigo: &str) {
        let _ = self.web.evaluate_script(codigo);
    }

    fn executar(&mut self, acao: Acao) {
        match acao {
            Acao::Mostrar => {
                self.posicionar();
                self.janela.set_visible(true);
                self.pedir_foco();
                self.js("window.energiaVisivel && window.energiaVisivel(true)");
            }
            Acao::Esconder => {
                self.js("window.energiaVisivel && window.energiaVisivel(false)");
                self.janela.set_visible(false);
            }
            Acao::Nada => {}
        }
    }

    /// Posiciona com o tamanho que a página pediu (o tamanho real só é conhecido depois de mapear).
    fn posicionar(&self) {
        let clique = self.estado.clique();
        let Some(area) = area_util(clique) else {
            return;
        };
        let tamanho = tamanho_fisico(LARGURA, self.estado.altura(), self.janela.scale_factor());
        let (x, y) = posicionar(clique, tamanho, area);
        self.janela.set_outer_position(PhysicalPosition::new(x, y));
    }

    /// No X11, pede o foco com o carimbo de tempo do servidor: sem ele o gerenciador de janelas
    /// recusa (proteção contra roubo de foco) e clicar fora não fecharia o popup.
    fn pedir_foco(&self) {
        let janela_gtk = self.janela.gtk_window();
        let x11 = janela_gtk
            .window()
            .and_then(|j| j.downcast::<gdkx11::X11Window>().ok());
        match x11 {
            Some(j) => janela_gtk.present_with_time(gdkx11::functions::x11_get_server_time(&j)),
            None => self.janela.set_focus(),
        }
    }

    pub fn mostrar(&mut self, clique: Option<(i32, i32)>) {
        let acao = self.estado.mostrar(clique, Instant::now());
        self.executar(acao);
    }

    pub fn esconder(&mut self) {
        let acao = self.estado.esconder();
        self.executar(acao);
    }

    pub fn alternar(&mut self, clique: Option<(i32, i32)>) {
        let acao = self.estado.alternar(clique, Instant::now());
        self.executar(acao);
    }

    pub fn perdeu_foco(&mut self) {
        let acao = self.estado.perdeu_foco(Instant::now());
        self.executar(acao);
    }

    /// A página mediu o conteúdo: ajusta a altura e, se estiver na tela, reposiciona.
    pub fn ajustar_altura(&mut self, altura: u32) {
        let reposicionar = self.estado.nova_altura(altura);
        self.janela
            .set_inner_size(LogicalSize::new(LARGURA, f64::from(altura)));
        if reposicionar {
            self.posicionar();
        }
    }

    /// Qualquer mensagem da página prova que ela carregou.
    pub fn mensagem_recebida(&mut self) {
        self.estado.mensagem_recebida();
    }

    /// O serviço caiu ou voltou: recarrega o popup (ele mostra "parado" sozinho sem dados).
    pub fn servico(&mut self, ok: bool) {
        self.servico_ok = ok;
        if self.estado.servico(ok).is_some() {
            let _ = self.web.load_url(conteudo::PAGINA);
        }
    }

    /// Chamado periodicamente: se o /mini não deu sinal de vida, tenta carregar de novo.
    pub fn checar_carga(&mut self) {
        if self.estado.precisa_recarregar() {
            self.estado.recarregou();
            tracing::debug!("o popup não respondeu; recarregando");
            let _ = self.web.load_url(conteudo::PAGINA);
        }
    }
}
