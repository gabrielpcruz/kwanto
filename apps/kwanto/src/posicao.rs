//! Onde o popup aparece: junto ao clique, abaixo de barras no topo e acima de painéis embaixo,
//! sempre inteiro dentro do monitor.

pub const MARGEM: i32 = 8;
/// Distância entre o ponto do clique e o popup.
const AFASTAMENTO: i32 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retangulo {
    pub x: i32,
    pub y: i32,
    pub largura: u32,
    pub altura: u32,
}

fn dentro(v: i32, min: i32, max: i32) -> i32 {
    if max < min { min } else { v.clamp(min, max) }
}

fn i32_de(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

/// Canto superior esquerdo do popup. `clique` (0,0) ou ausente = canto superior direito.
#[must_use]
pub fn posicionar(clique: Option<(i32, i32)>, janela: (u32, u32), area: Retangulo) -> (i32, i32) {
    let (w, h) = (i32_de(janela.0), i32_de(janela.1));
    let (aw, ah) = (i32_de(area.largura), i32_de(area.altura));
    let max_x = area.x + aw - w - MARGEM;
    let max_y = area.y + ah - h - MARGEM;
    let (x, y) = match clique {
        None | Some((0, 0)) => (max_x, area.y + MARGEM),
        Some((cx, cy)) => {
            let y = if cy < area.y + ah / 2 {
                cy + AFASTAMENTO
            } else {
                cy - h - AFASTAMENTO
            };
            (cx - w / 2, y)
        }
    };
    (
        dentro(x, area.x + MARGEM, max_x),
        dentro(y, area.y + MARGEM, max_y),
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    const TELA: Retangulo = Retangulo {
        x: 0,
        y: 0,
        largura: 2560,
        altura: 1080,
    };

    #[test]
    fn barra_em_cima_abre_abaixo_centralizado_no_clique() {
        assert_eq!(posicionar(Some((1000, 15)), (360, 480), TELA), (820, 29));
    }

    #[test]
    fn painel_embaixo_abre_acima() {
        assert_eq!(posicionar(Some((1000, 1065)), (360, 480), TELA), (820, 571));
    }

    #[test]
    fn clique_na_borda_direita_fica_dentro_da_tela() {
        assert_eq!(posicionar(Some((2550, 15)), (360, 480), TELA), (2192, 29));
    }

    #[test]
    fn sem_coordenadas_vai_para_o_canto_superior_direito() {
        assert_eq!(posicionar(None, (360, 480), TELA), (2192, 8));
        assert_eq!(posicionar(Some((0, 0)), (360, 480), TELA), (2192, 8));
    }

    #[test]
    fn segundo_monitor_e_janela_maior_que_a_area() {
        let m2 = Retangulo {
            x: 2560,
            y: 0,
            largura: 1920,
            altura: 300,
        };
        let (x, y) = posicionar(Some((3000, 15)), (360, 480), m2);
        assert_eq!(x, 2820);
        assert_eq!(y, 8); // não cabe: encosta no topo em vez de sair da tela
    }
}
