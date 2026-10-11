//! Componentes do PC e o que um medidor informa sobre eles.
use crate::unidades::Watts;
use serde::{Deserialize, Serialize};

/// Identificador estável de um componente (ex.: `cpu_nucleos`, `gpu:GPU-uuid`, `disco:naa.5000…`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComponenteId(String);

impl ComponenteId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// ID que não revela o identificador do hardware (WWID/serial do disco, UUID da GPU): o
    /// prefixo seguido de 16 dígitos hexadecimais de SHA-256. Estável entre reinícios.
    #[must_use]
    pub fn anonimo(prefixo: &str, bruto: &str) -> Self {
        use sha2::{Digest, Sha256};
        let resumo = Sha256::digest(format!("kwanto/{prefixo}/{bruto}"));
        let hex: String = resumo.iter().take(8).map(|b| format!("{b:02x}")).collect();
        Self(format!("{prefixo}:id-{hex}"))
    }

    /// Para IDs gravados por versões antigas que ainda contêm o identificador do hardware:
    /// `(prefixo, identificador)`. `None` se o ID já é anônimo ou nunca teve identificador.
    #[must_use]
    pub fn bruto_identificavel(&self) -> Option<(&str, &str)> {
        let anonimo = |r: &str| {
            r.strip_prefix("id-")
                .is_some_and(|h| h.len() == 16 && h.chars().all(|c| c.is_ascii_hexdigit()))
        };
        if let Some(r) = self.0.strip_prefix("disco:") {
            return (!anonimo(r)).then_some(("disco", r));
        }
        self.0
            .strip_prefix("gpu:")
            .filter(|r| r.starts_with("GPU-"))
            .map(|r| ("gpu", r))
    }

    /// Componente calculado que representa a perda de energia na fonte.
    #[must_use]
    pub fn perda_fonte() -> Self {
        Self::new("fonte_perda")
    }
}

/// Como a potência de um componente é obtida; define o tipo e o modelo de estimativa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Perfil {
    Sensor,
    Cpu,
    Hdd,
    Ssd,
    Ram,
    PlacaMae,
    FansUsb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TipoComponente {
    Medido,
    Estimado,
    Calculado,
}

impl TipoComponente {
    #[must_use]
    pub fn como_texto(self) -> &'static str {
        match self {
            Self::Medido => "medido",
            Self::Estimado => "estimado",
            Self::Calculado => "calculado",
        }
    }

    /// Converte o texto gravado no catálogo; valores desconhecidos viram `Estimado`.
    #[must_use]
    pub fn de_texto(texto: &str) -> Self {
        match texto {
            "medido" => Self::Medido,
            "calculado" => Self::Calculado,
            _ => Self::Estimado,
        }
    }
}

/// Componente detectado por um medidor.
#[derive(Debug, Clone, PartialEq)]
pub struct Componente {
    pub id: ComponenteId,
    pub rotulo: String,
    /// De onde vem o número, em linguagem do usuário ("RAPL do kernel", "valor fixo"…).
    pub origem: String,
    pub perfil: Perfil,
}

/// O que fica gravado no catálogo e aparece na API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DescricaoComponente {
    pub id: ComponenteId,
    pub rotulo: String,
    pub tipo: TipoComponente,
    pub origem: String,
}

/// Tamanho máximo de um rótulo, em caracteres.
pub const ROTULO_MAX: usize = 64;

/// Texto seguro para mostrar: rótulos vêm do hardware (sysfs, NVML), que pode ser forjado por um
/// dispositivo conectado. Tira caracteres com significado em HTML/JS, troca controles por espaço,
/// junta espaços e limita o tamanho.
#[must_use]
pub fn rotulo_seguro(texto: &str) -> String {
    let sem_perigo: String = texto
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | '"' | '\'' | '&' | '`'))
        .map(|c| {
            if c.is_control() || c.is_whitespace() {
                ' '
            } else {
                c
            }
        })
        .collect();
    sem_perigo
        .split(' ')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(ROTULO_MAX)
        .collect()
}

impl Componente {
    #[must_use]
    pub fn novo(id: &str, rotulo: &str, origem: &str, perfil: Perfil) -> Self {
        Self {
            id: ComponenteId::new(id),
            rotulo: rotulo_seguro(rotulo),
            origem: rotulo_seguro(origem),
            perfil,
        }
    }

    #[must_use]
    pub fn tipo(&self) -> TipoComponente {
        if self.perfil == Perfil::Sensor {
            TipoComponente::Medido
        } else {
            TipoComponente::Estimado
        }
    }

    #[must_use]
    pub fn descricao(&self) -> DescricaoComponente {
        DescricaoComponente {
            id: self.id.clone(),
            rotulo: self.rotulo.clone(),
            tipo: self.tipo(),
            origem: self.origem.clone(),
        }
    }
}

/// Resultado de uma leitura de um medidor, antes da estimativa.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Amostra {
    /// Potência medida diretamente por sensor.
    Potencia(Watts),
    /// Fração de tempo ocupado, de 0 a 1.
    Atividade(f64),
    /// Disco em repouso (motor parado).
    Standby,
    /// Sem medição; usa o valor fixo configurado.
    Fixo,
    /// O sensor não respondeu nesta leitura.
    SemLeitura,
}

#[cfg(test)]
mod testes_id {
    use super::*;

    #[test]
    fn id_anonimo_nao_contem_o_identificador() {
        let id = ComponenteId::anonimo("disco", "naa.0000000000000001");
        assert!(id.as_str().starts_with("disco:id-"), "{}", id.as_str());
        assert_eq!(id.as_str().len(), "disco:id-".len() + 16);
        assert!(!id.as_str().contains("0000000000000001"));
        assert_eq!(
            id,
            ComponenteId::anonimo("disco", "naa.0000000000000001"),
            "estável"
        );
        assert_ne!(id, ComponenteId::anonimo("disco", "naa.0000000000000002"));
        assert_ne!(id, ComponenteId::anonimo("gpu", "naa.0000000000000001"));
    }

    #[test]
    fn sabe_quais_ids_antigos_precisam_de_anonimizacao() {
        assert_eq!(
            ComponenteId::new("disco:naa.5000c500").bruto_identificavel(),
            Some(("disco", "naa.5000c500"))
        );
        assert_eq!(
            ComponenteId::new("gpu:GPU-abc").bruto_identificavel(),
            Some(("gpu", "GPU-abc"))
        );
        let ja = ComponenteId::anonimo("disco", "x");
        assert_eq!(ja.bruto_identificavel(), None, "já anônimo");
        for id in ["gpu:0000:01:00.0", "cpu_nucleos", "ram", "fonte_perda"] {
            assert_eq!(ComponenteId::new(id).bruto_identificavel(), None, "{id}");
        }
    }
}

#[cfg(test)]
mod testes_rotulo {
    use super::*;

    #[test]
    fn rotulo_hostil_vira_texto_inofensivo() {
        let r = rotulo_seguro("HDD <img src=x onerror=import('//ab.cd')>");
        for c in ['<', '>', '"', '\'', '&', '`'] {
            assert!(!r.contains(c), "{r}");
        }
        assert!(r.starts_with("HDD img src=x onerror=import"));
    }

    #[test]
    fn controles_e_espacos_colapsam() {
        assert_eq!(
            rotulo_seguro("  SSD\u{0}\u{1b}[31m  Exemplo\t\n 512  "),
            "SSD [31m Exemplo 512"
        );
    }

    #[test]
    fn acentos_ficam_e_tamanho_e_limitado() {
        assert_eq!(rotulo_seguro("Placa-mãe"), "Placa-mãe");
        let longo = rotulo_seguro(&"é".repeat(4096));
        assert_eq!(longo.chars().count(), 64);
        assert_eq!(rotulo_seguro("<>\"'&`"), "");
    }

    #[test]
    fn componente_novo_limpa_rotulo_e_origem() {
        let c = Componente::novo("disco:x", "HDD <b>x</b>", "atividade <i>", Perfil::Hdd);
        assert_eq!(c.rotulo, "HDD bx/b");
        assert_eq!(c.origem, "atividade i");
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn tipo_vem_do_perfil() {
        let gpu = Componente::novo("gpu:x", "GPU", "NVML (driver NVIDIA)", Perfil::Sensor);
        let ram = Componente::novo("ram", "RAM", "valor fixo", Perfil::Ram);
        assert_eq!(gpu.tipo(), TipoComponente::Medido);
        assert_eq!(ram.tipo(), TipoComponente::Estimado);
    }

    #[test]
    fn tipo_serializa_em_minusculas() {
        let json = serde_json::to_string(&TipoComponente::Calculado).unwrap();
        assert_eq!(json, "\"calculado\"");
    }

    #[test]
    fn id_da_perda_da_fonte() {
        assert_eq!(ComponenteId::perda_fonte().as_str(), "fonte_perda");
    }
}
