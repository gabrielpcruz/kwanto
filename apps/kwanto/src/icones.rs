//! Ícones do item de bandeja, renderizados dos SVGs do projeto e enviados como pixmaps
//! (não dependem do tema de ícones do sistema).
use resvg::{tiny_skia, usvg};

/// (largura, altura, pixels ARGB32 em ordem de rede), como o StatusNotifierItem pede.
pub type Pixmap = (i32, i32, Vec<u8>);

const TAMANHOS: [u32; 4] = [16, 22, 32, 48];
const SVG_ATIVO: &[u8] = include_bytes!("../../../web/static/icons/kwanto.svg");
const SVG_DESLIGADO: &[u8] = include_bytes!("../../../web/static/icons/kwanto-off.svg");

#[derive(Debug, Clone)]
pub struct Icones {
    ativo: Vec<Pixmap>,
    desligado: Vec<Pixmap>,
}

#[allow(clippy::cast_precision_loss)]
fn renderizar(svg: &[u8], lado: u32) -> Option<Pixmap> {
    let arvore = usvg::Tree::from_data(svg, &usvg::Options::default()).ok()?;
    let mut tela = tiny_skia::Pixmap::new(lado, lado)?;
    let escala = lado as f32 / arvore.size().width().max(arvore.size().height());
    resvg::render(
        &arvore,
        tiny_skia::Transform::from_scale(escala, escala),
        &mut tela.as_mut(),
    );
    let argb = tela
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.alpha(), c.red(), c.green(), c.blue()]
        })
        .collect();
    let lado = i32::try_from(lado).ok()?;
    Some((lado, lado, argb))
}

impl Icones {
    #[must_use]
    pub fn embutidos() -> Self {
        let todos = |svg| {
            TAMANHOS
                .iter()
                .filter_map(|&t| renderizar(svg, t))
                .collect()
        };
        Self {
            ativo: todos(SVG_ATIVO),
            desligado: todos(SVG_DESLIGADO),
        }
    }

    #[must_use]
    pub fn para(&self, ativo: bool) -> Vec<Pixmap> {
        if ativo {
            self.ativo.clone()
        } else {
            self.desligado.clone()
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn gera_quatro_tamanhos_argb() {
        let ic = Icones::embutidos();
        let p = ic.para(true);
        let tamanhos: Vec<i32> = p.iter().map(|(w, _, _)| *w).collect();
        assert_eq!(tamanhos, vec![16, 22, 32, 48]);
        for (w, h, dados) in &p {
            assert_eq!(dados.len(), usize::try_from(w * h * 4).unwrap());
        }
    }

    #[test]
    fn ativo_e_desligado_sao_diferentes_e_tem_pixels_visiveis() {
        let ic = Icones::embutidos();
        let (a, d) = (ic.para(true), ic.para(false));
        assert_ne!(a[3].2, d[3].2);
        // ARGB: o primeiro byte de cada pixel é o alfa
        assert!(a[3].2.chunks(4).any(|px| px[0] > 200));
    }
}
