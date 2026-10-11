//! Coleta: lê os medidores a cada 2 s, estima, publica o estado ao vivo e acumula por minuto.
use kwanto_core::componente::{Amostra, Componente, ComponenteId, Perfil};
use kwanto_core::config::Config;
use kwanto_core::estimativa::{ParametrosDeEstimativa, estimar};
use kwanto_core::eventos::RegistroMinuto;
use kwanto_core::relogio::Relogio;
use kwanto_core::sensores::Medidor;
use kwanto_core::unidades::{WattHora, Watts};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

pub const INTERVALO_AMOSTRA: Duration = Duration::from_secs(2);
/// Intervalo maior que isso entre leituras significa que o PC estava suspenso.
pub const LIMITE_SUSPENSAO: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EstadoAoVivo {
    pub instante: Option<jiff::Timestamp>,
    pub watts: BTreeMap<ComponenteId, Watts>,
    pub atividade: BTreeMap<ComponenteId, f64>,
    pub faltando: Vec<ComponenteId>,
}

/// Soma a energia de cada componente dentro do minuto de parede.
#[derive(Debug, Default)]
pub struct Acumulador {
    aberto: Option<RegistroMinuto>,
}

impl Acumulador {
    /// Acrescenta uma amostra. Se o minuto mudou (para frente ou para trás), devolve o anterior fechado.
    pub fn adicionar(
        &mut self,
        minuto: i64,
        watts: &BTreeMap<ComponenteId, Watts>,
        dt: Duration,
    ) -> Option<RegistroMinuto> {
        let fechado = match &self.aberto {
            Some(r) if r.minuto != minuto => self.aberto.take(),
            _ => None,
        };
        let r = self.aberto.get_or_insert_with(|| RegistroMinuto {
            minuto,
            ..RegistroMinuto::default()
        });
        for (id, w) in watts {
            *r.energia.entry(id.clone()).or_insert(WattHora(0.0)) += w.durante(dt);
        }
        r.segundos_ligado += dt.as_secs_f64();
        fechado
    }

    /// Fecha o minuto aberto (usado no desligamento).
    pub fn esvaziar(&mut self) -> Option<RegistroMinuto> {
        self.aberto.take()
    }
}

pub struct Amostrador {
    medidores: Vec<Box<dyn Medidor>>,
    perfis: HashMap<ComponenteId, Perfil>,
    relogio: Arc<dyn Relogio>,
    config: watch::Receiver<Config>,
    estado: watch::Sender<Arc<EstadoAoVivo>>,
    acumulador: Acumulador,
    ultimo: Duration,
}

impl std::fmt::Debug for Amostrador {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Amostrador")
            .field("medidores", &self.medidores.len())
            .finish_non_exhaustive()
    }
}

fn minuto_de(agora: &jiff::Zoned) -> i64 {
    agora.timestamp().as_second().div_euclid(60) * 60
}

impl Amostrador {
    #[must_use]
    pub fn novo(
        medidores: Vec<Box<dyn Medidor>>,
        relogio: Arc<dyn Relogio>,
        config: watch::Receiver<Config>,
        estado: watch::Sender<Arc<EstadoAoVivo>>,
    ) -> Self {
        let perfis = medidores
            .iter()
            .flat_map(|m| m.componentes())
            .map(|c| (c.id, c.perfil))
            .collect();
        let ultimo = relogio.intervalo();
        Self {
            medidores,
            perfis,
            relogio,
            config,
            estado,
            acumulador: Acumulador::default(),
            ultimo,
        }
    }

    #[must_use]
    pub fn componentes(&self) -> Vec<Componente> {
        self.medidores
            .iter()
            .flat_map(|m| m.componentes())
            .collect()
    }

    /// Uma leitura completa. Devolve o minuto que acabou de fechar, se houver.
    pub fn passo(&mut self) -> Option<RegistroMinuto> {
        let agora_intervalo = self.relogio.intervalo();
        let dt = agora_intervalo.saturating_sub(self.ultimo);
        self.ultimo = agora_intervalo;
        if dt > LIMITE_SUSPENSAO {
            tracing::info!(
                segundos = dt.as_secs(),
                "intervalo longo (suspensão); leitura descartada"
            );
            self.medidores.iter_mut().for_each(|m| m.reiniciar());
            return None;
        }
        if dt.is_zero() {
            return None;
        }
        let parametros = ParametrosDeEstimativa::de_config(&self.config.borrow());
        let mut estado = EstadoAoVivo::default();
        for medidor in &mut self.medidores {
            for (id, amostra) in medidor.ler(dt) {
                let perfil = self.perfis.get(&id).copied().unwrap_or(Perfil::Sensor);
                match amostra {
                    Amostra::Atividade(f) => {
                        estado.atividade.insert(id.clone(), f.clamp(0.0, 1.0));
                    }
                    Amostra::Standby => {
                        estado.atividade.insert(id.clone(), 0.0);
                    }
                    _ => {}
                }
                let w = estimar(perfil, amostra, &parametros).unwrap_or_else(|| {
                    estado.faltando.push(id.clone());
                    Watts(0.0)
                });
                estado.watts.insert(id, w);
            }
        }
        let agora = self.relogio.agora();
        estado.instante = Some(agora.timestamp());
        let fechado = self
            .acumulador
            .adicionar(minuto_de(&agora), &estado.watts, dt);
        self.estado.send_replace(Arc::new(estado));
        fechado
    }

    /// Fecha o minuto parcial (desligamento do serviço).
    pub fn encerrar(&mut self) -> Option<RegistroMinuto> {
        self.acumulador.esvaziar()
    }
}
