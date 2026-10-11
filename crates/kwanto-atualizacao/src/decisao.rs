//! Oferecer ou não a atualização.
use crate::ErroAtualizacao;
use crate::formato::FormatoPacote;
use crate::manifesto::{Manifesto, Pacote};
use crate::versao::Versao;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decisao {
    Atualizar { versao: Versao, pacote: Pacote },
    JaAtualizado,
    SemPacoteParaFormato,
}

pub fn decidir(
    m: &Manifesto,
    atual: Versao,
    formato: FormatoPacote,
) -> Result<Decisao, ErroAtualizacao> {
    let versao = m.versao()?;
    if versao <= atual {
        return Ok(Decisao::JaAtualizado);
    }
    Ok(match m.pacote(formato) {
        Some(p) => Decisao::Atualizar {
            versao,
            pacote: p.clone(),
        },
        None => Decisao::SemPacoteParaFormato,
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    fn manifesto(versao: &str, com_rpm: bool) -> Manifesto {
        let p = |a: &str| Pacote {
            arquivo: a.into(),
            sha256: "a".repeat(64),
            tamanho: 10,
        };
        let mut pacotes = std::collections::BTreeMap::new();
        pacotes.insert("deb".to_string(), p("kwanto_x_amd64.deb"));
        if com_rpm {
            pacotes.insert("rpm".to_string(), p("kwanto-x-1.x86_64.rpm"));
        }
        Manifesto {
            versao: versao.into(),
            pacotes,
        }
    }
    fn v(t: &str) -> Versao {
        Versao::ler(t).unwrap()
    }

    #[test]
    fn versao_maior_atualiza() {
        let d = decidir(&manifesto("0.2.0", true), v("0.1.0"), FormatoPacote::Rpm).unwrap();
        assert!(
            matches!(d, Decisao::Atualizar { versao, pacote } if versao == v("0.2.0") && pacote.arquivo.ends_with(".rpm"))
        );
    }

    #[test]
    fn igual_ou_menor_nao_atualiza() {
        assert_eq!(
            decidir(&manifesto("0.1.0", true), v("0.1.0"), FormatoPacote::Deb).unwrap(),
            Decisao::JaAtualizado
        );
        assert_eq!(
            decidir(&manifesto("0.0.9", true), v("0.1.0"), FormatoPacote::Deb).unwrap(),
            Decisao::JaAtualizado
        );
    }

    #[test]
    fn sem_pacote_do_formato() {
        assert_eq!(
            decidir(&manifesto("0.2.0", false), v("0.1.0"), FormatoPacote::Rpm).unwrap(),
            Decisao::SemPacoteParaFormato
        );
    }
}
