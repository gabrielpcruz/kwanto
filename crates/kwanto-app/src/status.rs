//! Monta o status completo servido em /api/status (mesmo contrato da versão Python).
use crate::coleta::EstadoAoVivo;
use crate::config_uc::config_como_json;
use crate::portas::{CatalogoComponentes, ErroRepositorio, Intervalo, RepositorioEnergia};
use kwanto_core::componente::{ComponenteId, DescricaoComponente, TipoComponente, rotulo_seguro};
use kwanto_core::config::{Config, ESQUEMA};
use kwanto_core::conta::{CicloDaConta, fator_perda, prever};
use kwanto_core::relogio::Relogio;
use kwanto_core::unidades::WattHora;
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::watch;

pub const VERSAO_API: u32 = 1;
const SETE_DIAS: i64 = 7 * 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InfoHardware {
    pub cpu_estimada: bool,
    pub tem_hdd: bool,
}

#[derive(Debug, Serialize)]
pub struct ComponenteAgora {
    pub id: String,
    pub rotulo: String,
    pub tipo: TipoComponente,
    pub origem: String,
    #[serde(serialize_with = "crate::arredondar::uma")]
    pub w: f64,
    #[serde(serialize_with = "crate::arredondar::uma")]
    pub pct: f64,
    #[serde(serialize_with = "crate::arredondar::opcional_duas")]
    pub atividade: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ComponentePeriodo {
    pub id: String,
    pub rotulo: String,
    pub tipo: TipoComponente,
    pub origem: String,
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub kwh: f64,
    #[serde(serialize_with = "crate::arredondar::duas")]
    pub custo: f64,
    #[serde(serialize_with = "crate::arredondar::uma")]
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct Agora {
    #[serde(serialize_with = "crate::arredondar::uma")]
    pub parede_w: f64,
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub custo_hora: f64,
    #[serde(serialize_with = "crate::arredondar::opcional_uma")]
    pub atualizado_ha_s: Option<f64>,
    pub componentes: Vec<ComponenteAgora>,
}

#[derive(Debug, Serialize)]
pub struct Periodo {
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub kwh: f64,
    #[serde(serialize_with = "crate::arredondar::duas")]
    pub custo: f64,
    pub componentes: Vec<ComponentePeriodo>,
}

#[derive(Debug, Serialize)]
pub struct Ciclo {
    pub inicio: String,
    pub fim: String,
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub kwh: f64,
    #[serde(serialize_with = "crate::arredondar::duas")]
    pub custo: f64,
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub previsao_kwh: f64,
    #[serde(serialize_with = "crate::arredondar::duas")]
    pub previsao_custo: f64,
    pub componentes: Vec<ComponentePeriodo>,
}

#[derive(Debug, Serialize)]
pub struct Media {
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub kwh_dia: f64,
    #[serde(serialize_with = "crate::arredondar::duas")]
    pub custo_dia: f64,
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub horas_ligado_dia: f64,
    #[serde(serialize_with = "crate::arredondar::duas")]
    pub janela_dias: f64,
}

#[derive(Debug, Serialize)]
pub struct Estimativa {
    #[serde(serialize_with = "crate::arredondar::tres")]
    pub kwh: f64,
    #[serde(serialize_with = "crate::arredondar::duas")]
    pub custo: f64,
}

#[derive(Debug, Serialize)]
pub struct ItemEsquemaJson {
    pub min: f64,
    pub max: f64,
    pub rotulo: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub versao_api: u32,
    pub agora: Agora,
    pub hoje: Periodo,
    pub ciclo: Ciclo,
    pub media: Media,
    pub estimativa_24x7: Estimativa,
    pub config: Map<String, Value>,
    pub config_schema: BTreeMap<&'static str, ItemEsquemaJson>,
    pub cpu_estimada: bool,
    pub tarifa_efetiva: f64,
    pub tem_hdd: bool,
    pub avisos: Vec<String>,
}

pub struct ConsultarStatus {
    energia: Arc<dyn RepositorioEnergia>,
    catalogo: Arc<dyn CatalogoComponentes>,
    relogio: Arc<dyn Relogio>,
    config: watch::Receiver<Config>,
    estado: watch::Receiver<Arc<EstadoAoVivo>>,
    info: InfoHardware,
    /// Somas do banco do minuto atual: o banco só muda uma vez por minuto.
    cache: Mutex<Option<(ChaveSomas, Arc<Somas>)>>,
}

/// (minuto, início do dia, início do ciclo): muda o minuto, o dia ou o ciclo, recalcula.
type ChaveSomas = (i64, i64, i64);

/// Energia DC (sem a perda da fonte) lida do banco para montar o status.
#[derive(Debug)]
struct Somas {
    hoje: BTreeMap<ComponenteId, WattHora>,
    ciclo: BTreeMap<ComponenteId, WattHora>,
    janela: BTreeMap<ComponenteId, WattHora>,
    janela_ini: i64,
    ligado_s: f64,
}

impl std::fmt::Debug for ConsultarStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsultarStatus")
            .field("info", &self.info)
            .finish_non_exhaustive()
    }
}

type Catalogo = HashMap<ComponenteId, DescricaoComponente>;

/// Acrescenta a perda da fonte (componente calculado) a um conjunto de valores DC.
fn com_perda<T: Copy + Into<f64>>(
    dc: &BTreeMap<ComponenteId, T>,
    eficiencia: f64,
) -> BTreeMap<ComponenteId, f64> {
    let mut m: BTreeMap<ComponenteId, f64> =
        dc.iter().map(|(k, v)| (k.clone(), (*v).into())).collect();
    let total: f64 = m.values().sum();
    m.insert(ComponenteId::perda_fonte(), total * fator_perda(eficiencia));
    m
}

fn descrever(id: &ComponenteId, cat: &Catalogo) -> (String, TipoComponente, String) {
    if *id == ComponenteId::perda_fonte() {
        return (
            "Perda na fonte".into(),
            TipoComponente::Calculado,
            "eficiência da fonte".into(),
        );
    }
    // Limpa também na saída: o catálogo pode ter rótulos gravados antes da limpeza na entrada.
    cat.get(id).map_or_else(
        || {
            (
                rotulo_seguro(id.as_str()),
                TipoComponente::Estimado,
                String::new(),
            )
        },
        |d| (rotulo_seguro(&d.rotulo), d.tipo, rotulo_seguro(&d.origem)),
    )
}

fn ordenados(m: &BTreeMap<ComponenteId, f64>) -> Vec<(&ComponenteId, f64)> {
    let mut v: Vec<_> = m.iter().map(|(k, v)| (k, *v)).collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
    v
}

fn kwh(m: &BTreeMap<ComponenteId, f64>) -> f64 {
    m.values().sum::<f64>() / 1000.0
}

fn periodo(wh: &BTreeMap<ComponenteId, f64>, preco: f64, cat: &Catalogo) -> Vec<ComponentePeriodo> {
    let total: f64 = wh.values().sum::<f64>().max(f64::MIN_POSITIVE);
    ordenados(wh)
        .into_iter()
        .map(|(id, v)| {
            let (rotulo, tipo, origem) = descrever(id, cat);
            ComponentePeriodo {
                id: id.as_str().into(),
                rotulo,
                tipo,
                origem,
                kwh: v / 1000.0,
                custo: v / 1000.0 * preco,
                pct: v / total * 100.0,
            }
        })
        .collect()
}

fn erro_tempo(e: jiff::Error) -> ErroRepositorio {
    ErroRepositorio::Armazenamento(format!("data inválida: {e}"))
}

impl ConsultarStatus {
    #[must_use]
    pub fn novo(
        energia: Arc<dyn RepositorioEnergia>,
        catalogo: Arc<dyn CatalogoComponentes>,
        relogio: Arc<dyn Relogio>,
        config: watch::Receiver<Config>,
        estado: watch::Receiver<Arc<EstadoAoVivo>>,
        info: InfoHardware,
    ) -> Self {
        Self {
            energia,
            catalogo,
            relogio,
            config,
            estado,
            info,
            cache: Mutex::new(None),
        }
    }

    /// Somas do banco, reaproveitadas dentro do mesmo minuto.
    fn somas(
        &self,
        ts: i64,
        meia_noite: i64,
        ciclo_ini: i64,
    ) -> Result<Arc<Somas>, ErroRepositorio> {
        let minuto = ts.div_euclid(60) * 60;
        let chave = (minuto, meia_noite, ciclo_ini);
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((c, somas)) = cache.as_ref() {
            if *c == chave {
                return Ok(Arc::clone(somas));
            }
        }
        let fim = minuto + 60;
        let totais = |inicio| {
            self.energia
                .totais_por_componente(Intervalo { inicio, fim })
        };
        let primeiro = self.energia.primeiro_minuto()?.unwrap_or(minuto);
        let janela_ini = (minuto - SETE_DIAS).max(primeiro);
        let somas = Arc::new(Somas {
            hoje: totais(meia_noite)?,
            ciclo: totais(ciclo_ini)?,
            janela: totais(janela_ini)?,
            janela_ini,
            ligado_s: self.energia.segundos_ligado(Intervalo {
                inicio: janela_ini,
                fim,
            })?,
        });
        *cache = Some((chave, Arc::clone(&somas)));
        Ok(somas)
    }

    fn agora(
        estado: &EstadoAoVivo,
        ef: f64,
        preco: f64,
        cat: &Catalogo,
        agora: &jiff::Zoned,
    ) -> Agora {
        let w = com_perda(&estado.watts, ef);
        let parede: f64 = w.values().sum();
        let componentes = ordenados(&w)
            .into_iter()
            .map(|(id, v)| {
                let (rotulo, tipo, origem) = descrever(id, cat);
                ComponenteAgora {
                    id: id.as_str().into(),
                    rotulo,
                    tipo,
                    origem,
                    w: v,
                    pct: v / parede.max(f64::MIN_POSITIVE) * 100.0,
                    atividade: estado.atividade.get(id).copied(),
                }
            })
            .collect();
        let atualizado_ha_s = estado
            .instante
            .map(|t| agora.timestamp().duration_since(t).as_secs_f64());
        Agora {
            parede_w: parede,
            custo_hora: parede / 1000.0 * preco,
            atualizado_ha_s,
            componentes,
        }
    }

    fn avisos(&self, estado: &EstadoAoVivo, cat: &Catalogo) -> Vec<String> {
        let mut avisos = Vec::new();
        if self.info.cpu_estimada {
            avisos.push(
                "A CPU está sendo estimada pelo uso, porque o sensor RAPL não está acessível (hardware sem RAPL, \
                 máquina virtual ou serviço sem root). Ajuste os watts dela em Tarifa e ajustes."
                    .to_string(),
            );
        }
        if !estado.faltando.is_empty() {
            let nomes: Vec<String> = estado
                .faltando
                .iter()
                .map(|id| descrever(id, cat).0)
                .collect();
            avisos.push(format!(
                "Sem leitura no momento: {}. Esses itens contam como 0 W até o sensor voltar.",
                nomes.join(", ")
            ));
        }
        avisos
    }

    #[allow(clippy::cast_precision_loss)]
    pub fn executar(&self) -> Result<Status, ErroRepositorio> {
        let config = self.config.borrow().clone();
        let estado = Arc::clone(&self.estado.borrow());
        let preco = config.tarifa().efetiva().0;
        let ef = config.eficiencia();
        let agora = self.relogio.agora();
        let ts = agora.timestamp().as_second();
        let cat: Catalogo = self
            .catalogo
            .listar()?
            .into_iter()
            .map(|d| (d.id.clone(), d))
            .collect();
        let agora_dto = Self::agora(&estado, ef, preco, &cat, &agora);
        let parede = agora_dto.parede_w;

        // hoje e ciclo
        let meia_noite = agora
            .start_of_day()
            .map_err(erro_tempo)?
            .timestamp()
            .as_second();
        let ciclo = CicloDaConta::contendo(agora.date(), config.dia_leitura());
        let tz = agora.time_zone().clone();
        let inicio_de =
            |d: jiff::civil::Date| d.to_zoned(tz.clone()).map(|z| z.timestamp().as_second());
        let ciclo_ini = inicio_de(ciclo.inicio).map_err(erro_tempo)?;
        let ciclo_fim = inicio_de(ciclo.fim).map_err(erro_tempo)?;
        let somas = self.somas(ts, meia_noite, ciclo_ini)?;
        let hoje_wh = com_perda(&somas.hoje, ef);
        let ciclo_wh = com_perda(&somas.ciclo, ef);

        // média dos últimos 7 dias de relógio (inclui tempo desligado)
        let janela_kwh = kwh(&com_perda(&somas.janela, ef));
        let ligado_s = somas.ligado_s;
        let janela_h = ((ts - somas.janela_ini) as f64 / 3600.0).max(1.0 / 60.0);
        let ciclo_kwh = kwh(&ciclo_wh);
        let previsao = prever(
            ciclo_kwh,
            janela_kwh,
            janela_h,
            (ciclo_fim - ts) as f64 / 3600.0,
        );

        Ok(Status {
            versao_api: VERSAO_API,
            agora: agora_dto,
            hoje: Periodo {
                kwh: kwh(&hoje_wh),
                custo: kwh(&hoje_wh) * preco,
                componentes: periodo(&hoje_wh, preco, &cat),
            },
            ciclo: Ciclo {
                inicio: ciclo.inicio.to_string(),
                fim: ciclo.fim.to_string(),
                kwh: ciclo_kwh,
                custo: ciclo_kwh * preco,
                previsao_kwh: previsao.previsto_kwh,
                previsao_custo: previsao.previsto_kwh * preco,
                componentes: periodo(&ciclo_wh, preco, &cat),
            },
            media: Media {
                kwh_dia: previsao.media_kwh_hora * 24.0,
                custo_dia: previsao.media_kwh_hora * 24.0 * preco,
                horas_ligado_dia: ligado_s / 3600.0 / janela_h * 24.0,
                janela_dias: janela_h / 24.0,
            },
            estimativa_24x7: Estimativa {
                kwh: parede * 720.0 / 1000.0,
                custo: parede * 720.0 / 1000.0 * preco,
            },
            config: config_como_json(&config),
            config_schema: ESQUEMA
                .iter()
                .map(|i| {
                    (
                        i.chave,
                        ItemEsquemaJson {
                            min: i.min,
                            max: i.max,
                            rotulo: i.rotulo,
                        },
                    )
                })
                .collect(),
            cpu_estimada: self.info.cpu_estimada,
            tarifa_efetiva: preco,
            tem_hdd: self.info.tem_hdd,
            avisos: self.avisos(&estado, &cat),
        })
    }
}
