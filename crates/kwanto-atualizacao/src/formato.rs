//! Qual gerenciador de pacotes instalou o Kwanto. Sem dono conhecido (manual, Arch), o
//! atualizador apenas avisa.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatoPacote {
    Deb,
    Rpm,
}

impl FormatoPacote {
    /// Chave do pacote no manifesto.
    #[must_use]
    pub fn chave(self) -> &'static str {
        match self {
            Self::Deb => "deb",
            Self::Rpm => "rpm",
        }
    }
}

pub trait Executor {
    /// Roda o programa (sem shell, saída descartada) e diz se terminou com sucesso.
    fn sucesso(&self, programa: &str, args: &[&str]) -> bool;
}

#[derive(Debug, Default)]
pub struct ExecutorReal;

impl Executor for ExecutorReal {
    fn sucesso(&self, programa: &str, args: &[&str]) -> bool {
        std::process::Command::new(programa)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

#[must_use]
pub fn detectar_formato(exec: &dyn Executor, caminho: &str) -> Option<FormatoPacote> {
    if exec.sucesso("dpkg-query", &["-S", caminho]) {
        Some(FormatoPacote::Deb)
    } else if exec.sucesso("rpm", &["-qf", caminho]) {
        Some(FormatoPacote::Rpm)
    } else {
        None
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::cell::RefCell;

    struct Falso {
        donos: Vec<&'static str>,
        chamadas: RefCell<Vec<String>>,
    }
    impl Executor for Falso {
        fn sucesso(&self, programa: &str, args: &[&str]) -> bool {
            self.chamadas
                .borrow_mut()
                .push(format!("{programa} {}", args.join(" ")));
            self.donos.contains(&programa)
        }
    }

    #[test]
    fn deb_rpm_e_nenhum() {
        let f = Falso {
            donos: vec!["dpkg-query"],
            chamadas: RefCell::default(),
        };
        assert_eq!(
            detectar_formato(&f, "/usr/bin/kwanto"),
            Some(FormatoPacote::Deb)
        );
        assert_eq!(f.chamadas.borrow()[0], "dpkg-query -S /usr/bin/kwanto");
        let f = Falso {
            donos: vec!["rpm"],
            chamadas: RefCell::default(),
        };
        assert_eq!(
            detectar_formato(&f, "/usr/bin/kwanto"),
            Some(FormatoPacote::Rpm)
        );
        assert_eq!(f.chamadas.borrow()[1], "rpm -qf /usr/bin/kwanto");
        let f = Falso {
            donos: vec![],
            chamadas: RefCell::default(),
        };
        assert_eq!(
            detectar_formato(&f, "/usr/bin/kwanto"),
            None,
            "Arch/manual: só avisar"
        );
    }

    #[test]
    fn chaves_do_manifesto() {
        assert_eq!(FormatoPacote::Deb.chave(), "deb");
        assert_eq!(FormatoPacote::Rpm.chave(), "rpm");
    }
}
