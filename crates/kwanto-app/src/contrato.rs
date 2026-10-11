//! Testes de contrato das portas: toda implementação (memória, SQLite) deve passar.
#![allow(clippy::unwrap_used)]
use crate::portas::{CatalogoComponentes, Intervalo, RepositorioConfig, RepositorioEnergia};
use kwanto_core::componente::{ComponenteId, DescricaoComponente, TipoComponente};
use kwanto_core::eventos::RegistroMinuto;
use kwanto_core::unidades::WattHora;

fn reg(minuto: i64, pares: &[(&str, f64)], seg: f64) -> RegistroMinuto {
    RegistroMinuto {
        minuto,
        energia: pares
            .iter()
            .map(|(k, v)| (ComponenteId::new(*k), WattHora(*v)))
            .collect(),
        segundos_ligado: seg,
    }
}

pub fn energia(repo: &dyn RepositorioEnergia) {
    assert_eq!(repo.primeiro_minuto().unwrap(), None);
    repo.gravar_minuto(&reg(600, &[("cpu", 1.0), ("gpu", 2.0)], 60.0))
        .unwrap();
    repo.gravar_minuto(&reg(660, &[("cpu", 1.5)], 60.0))
        .unwrap();
    // serviço reiniciou dentro do mesmo minuto: soma, não sobrescreve
    repo.gravar_minuto(&reg(660, &[("cpu", 0.5)], 10.0))
        .unwrap();
    repo.gravar_minuto(&reg(4200, &[("cpu", 3.0)], 30.0))
        .unwrap();

    assert_eq!(repo.primeiro_minuto().unwrap(), Some(600));
    let t = repo
        .totais_por_componente(Intervalo {
            inicio: 600,
            fim: 720,
        })
        .unwrap();
    assert_eq!(t[&ComponenteId::new("cpu")], WattHora(3.0));
    assert_eq!(t[&ComponenteId::new("gpu")], WattHora(2.0));
    assert!(
        (repo
            .segundos_ligado(Intervalo {
                inicio: 600,
                fim: 720
            })
            .unwrap()
            - 130.0)
            .abs()
            < 1e-9
    );
    assert!(
        repo.totais_por_componente(Intervalo {
            inicio: 720,
            fim: 4200
        })
        .unwrap()
        .is_empty()
    );

    let b = repo
        .baldes(
            Intervalo {
                inicio: 0,
                fim: 10_000,
            },
            3600,
        )
        .unwrap();
    assert_eq!(b.len(), 2);
    assert_eq!(
        (b[0].inicio, b[0].wh_dc, b[0].segundos_ligado),
        (0, WattHora(5.0), 130.0)
    );
    assert_eq!(
        (b[1].inicio, b[1].wh_dc, b[1].segundos_ligado),
        (3600, WattHora(3.0), 30.0)
    );
}

pub fn config(repo: &dyn RepositorioConfig) {
    assert!(repo.carregar().unwrap().is_empty());
    repo.salvar(&[("preco_kwh", 0.9), ("ram_w", 4.0)]).unwrap();
    repo.salvar(&[("preco_kwh", 1.1)]).unwrap();
    let mut v = repo.carregar().unwrap();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        v,
        vec![("preco_kwh".to_string(), 1.1), ("ram_w".to_string(), 4.0)]
    );
}

pub fn catalogo(repo: &dyn CatalogoComponentes) {
    let d = |id: &str, rotulo: &str| DescricaoComponente {
        id: ComponenteId::new(id),
        rotulo: rotulo.into(),
        tipo: TipoComponente::Medido,
        origem: "RAPL do kernel".into(),
    };
    repo.registrar(&[d("cpu", "CPU"), d("gpu:x", "GPU")])
        .unwrap();
    repo.registrar(&[d("cpu", "CPU nova")]).unwrap(); // atualiza; gpu:x continua
    let mut l = repo.listar().unwrap();
    l.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(l.len(), 2);
    assert_eq!(l[0].rotulo, "CPU nova");
}
