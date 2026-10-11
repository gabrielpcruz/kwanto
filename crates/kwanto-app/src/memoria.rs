//! Implementação em memória das portas, para testes.
use crate::portas::{
    Balde, CatalogoComponentes, ErroRepositorio, Intervalo, RepositorioConfig, RepositorioEnergia,
};
use kwanto_core::componente::{ComponenteId, DescricaoComponente};
use kwanto_core::eventos::RegistroMinuto;
use kwanto_core::unidades::WattHora;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

#[derive(Debug, Default)]
pub struct RepositorioMemoria {
    energia: Mutex<BTreeMap<(i64, ComponenteId), WattHora>>,
    ligado: Mutex<BTreeMap<i64, f64>>,
    config: Mutex<BTreeMap<String, f64>>,
    catalogo: Mutex<BTreeMap<ComponenteId, DescricaoComponente>>,
    falhar: AtomicBool,
}

fn trava<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl RepositorioMemoria {
    /// Faz `gravar_minuto` falhar (simula disco cheio).
    pub fn falhar_gravacoes(&self, falhar: bool) {
        self.falhar.store(falhar, Ordering::SeqCst);
    }
}

impl RepositorioEnergia for RepositorioMemoria {
    fn gravar_minuto(&self, r: &RegistroMinuto) -> Result<(), ErroRepositorio> {
        if self.falhar.load(Ordering::SeqCst) {
            return Err(ErroRepositorio::Armazenamento(
                "disco cheio (simulado)".into(),
            ));
        }
        let mut e = trava(&self.energia);
        for (id, wh) in &r.energia {
            *e.entry((r.minuto, id.clone())).or_default() += *wh;
        }
        *trava(&self.ligado).entry(r.minuto).or_default() += r.segundos_ligado;
        Ok(())
    }

    fn totais_por_componente(
        &self,
        i: Intervalo,
    ) -> Result<BTreeMap<ComponenteId, WattHora>, ErroRepositorio> {
        let mut t = BTreeMap::new();
        for ((m, id), wh) in trava(&self.energia).iter() {
            if (i.inicio..i.fim).contains(m) {
                *t.entry(id.clone()).or_insert(WattHora(0.0)) += *wh;
            }
        }
        Ok(t)
    }

    fn segundos_ligado(&self, i: Intervalo) -> Result<f64, ErroRepositorio> {
        Ok(trava(&self.ligado)
            .range(i.inicio..i.fim)
            .map(|(_, s)| s)
            .sum())
    }

    fn primeiro_minuto(&self) -> Result<Option<i64>, ErroRepositorio> {
        Ok(trava(&self.ligado).keys().next().copied())
    }

    fn baldes(&self, i: Intervalo, passo: i64) -> Result<Vec<Balde>, ErroRepositorio> {
        let mut b: BTreeMap<i64, Balde> = BTreeMap::new();
        for ((m, _), wh) in trava(&self.energia).iter() {
            if (i.inicio..i.fim).contains(m) {
                let k = m.div_euclid(passo) * passo;
                b.entry(k)
                    .or_insert(Balde {
                        inicio: k,
                        wh_dc: WattHora(0.0),
                        segundos_ligado: 0.0,
                    })
                    .wh_dc += *wh;
            }
        }
        for (m, s) in trava(&self.ligado).range(i.inicio..i.fim) {
            if let Some(balde) = b.get_mut(&(m.div_euclid(passo) * passo)) {
                balde.segundos_ligado += s;
            }
        }
        Ok(b.into_values().collect())
    }
}

impl RepositorioConfig for RepositorioMemoria {
    fn carregar(&self) -> Result<Vec<(String, f64)>, ErroRepositorio> {
        Ok(trava(&self.config)
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect())
    }

    fn salvar(&self, pares: &[(&str, f64)]) -> Result<(), ErroRepositorio> {
        let mut c = trava(&self.config);
        for (k, v) in pares {
            c.insert((*k).to_string(), *v);
        }
        Ok(())
    }
}

impl CatalogoComponentes for RepositorioMemoria {
    fn registrar(&self, componentes: &[DescricaoComponente]) -> Result<(), ErroRepositorio> {
        let mut c = trava(&self.catalogo);
        for d in componentes {
            c.insert(d.id.clone(), d.clone());
        }
        Ok(())
    }

    fn listar(&self) -> Result<Vec<DescricaoComponente>, ErroRepositorio> {
        Ok(trava(&self.catalogo).values().cloned().collect())
    }
}
