//! Casos de uso do Kwanto: coleta, consultas e configuração.
pub mod arredondar;
pub mod coleta;
pub mod config_uc;
#[cfg(feature = "testes")]
pub mod contrato;
pub mod historico;
#[cfg(feature = "testes")]
pub mod memoria;
pub mod portas;
pub mod registrar;
pub mod status;
