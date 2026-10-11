//! Regras do ajudante que instala como root. Ele não confia em nada da sessão do usuário além
//! dos caminhos: copia para um diretório só seu e verifica a cópia.

use crate::download::conferir_pacote;
use crate::formato::FormatoPacote;
use crate::manifesto::verificar_assinatura;
use crate::versao::Versao;
use crate::{ErroAtualizacao, LIMITE_BYTES};
use std::path::{Path, PathBuf};

pub const CAMINHO_AJUDANTE: &str = "/usr/libexec/kwanto/kwanto-atualizar";
const LIMITE_MANIFESTO: u64 = 1024 * 1024;
const LIMITE_ASSINATURA: u64 = 64 * 1024;

pub trait SistemaAjudante {
    fn eh_root(&self) -> bool;
    /// Diretório novo, exclusivo do root (0700).
    fn criar_dir_privado(&self) -> Result<PathBuf, String>;
    /// Copia no máximo `limite` bytes de um arquivo comum; erro se for maior.
    fn copiar(&self, de: &Path, para: &Path, limite: u64) -> Result<(), String>;
    fn ler(&self, caminho: &Path) -> Result<Vec<u8>, String>;
    /// Gerenciador de pacotes dono do Kwanto neste sistema.
    fn formato(&self) -> Option<FormatoPacote>;
    fn instalar(&self, formato: FormatoPacote, pacote: &Path) -> Result<(), String>;
    fn remover_dir(&self, caminho: &Path);
}

fn recusado(m: impl Into<String>) -> ErroAtualizacao {
    ErroAtualizacao::Recusado(m.into())
}

/// args: [manifesto, assinatura, pacote]. Devolve a versão instalada.
pub fn executar_ajudante(
    args: &[String],
    sis: &dyn SistemaAjudante,
    chave: &str,
    versao_instalada: Versao,
) -> Result<Versao, ErroAtualizacao> {
    if !sis.eh_root() {
        return Err(recusado("o ajudante precisa rodar como administrador"));
    }
    let [manifesto, assinatura, pacote] = args else {
        return Err(recusado(
            "uso: kwanto-atualizar <manifesto> <assinatura> <pacote>",
        ));
    };
    let dir = sis.criar_dir_privado().map_err(recusado)?;
    let resultado = instalar_verificado(
        sis,
        &dir,
        [manifesto, assinatura, pacote],
        chave,
        versao_instalada,
    );
    sis.remover_dir(&dir);
    resultado
}

fn instalar_verificado(
    sis: &dyn SistemaAjudante,
    dir: &Path,
    [manifesto, assinatura, pacote]: [&String; 3],
    chave: &str,
    versao_instalada: Versao,
) -> Result<Versao, ErroAtualizacao> {
    let nome_pacote = Path::new(pacote)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| recusado("pacote sem nome"))?;
    let m_priv = dir.join("manifesto.json");
    let a_priv = dir.join("manifesto.json.minisig");
    let p_priv = dir.join(nome_pacote);
    sis.copiar(Path::new(manifesto), &m_priv, LIMITE_MANIFESTO)
        .map_err(recusado)?;
    sis.copiar(Path::new(assinatura), &a_priv, LIMITE_ASSINATURA)
        .map_err(recusado)?;
    sis.copiar(Path::new(pacote), &p_priv, LIMITE_BYTES)
        .map_err(recusado)?;

    // Daqui em diante, só as cópias privadas.
    let m_bytes = sis.ler(&m_priv).map_err(recusado)?;
    let assinatura = String::from_utf8(sis.ler(&a_priv).map_err(recusado)?)
        .map_err(|_| ErroAtualizacao::Assinatura)?;
    let m = verificar_assinatura(&m_bytes, &assinatura, chave)?;
    let versao = m.versao()?;
    if versao <= versao_instalada {
        return Err(recusado(format!(
            "a versão {versao} não é mais nova que a instalada ({versao_instalada})"
        )));
    }
    let formato = sis
        .formato()
        .ok_or_else(|| recusado("não sei qual gerenciador de pacotes instalou o Kwanto"))?;
    let esperado = m
        .pacote(formato)
        .ok_or_else(|| recusado("o manifesto não tem pacote para este sistema"))?;
    if esperado.arquivo != nome_pacote {
        return Err(recusado(format!(
            "o pacote recebido ({nome_pacote}) não é o do manifesto ({})",
            esperado.arquivo
        )));
    }
    conferir_pacote(&sis.ler(&p_priv).map_err(recusado)?, esperado)?;
    sis.instalar(formato, &p_priv)
        .map_err(|e| recusado(format!("a instalação falhou: {e}")))?;
    Ok(versao)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::download::sha256_hex;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::io::Cursor;

    /// Sistema falso: arquivos em memória; registra o que foi instalado.
    struct Falso {
        root: bool,
        arquivos: RefCell<HashMap<PathBuf, Vec<u8>>>,
        formato: Option<FormatoPacote>,
        instalado: RefCell<Option<PathBuf>>,
        trocar_original_apos_copia: Option<Vec<u8>>,
        removido: RefCell<bool>,
    }
    impl SistemaAjudante for Falso {
        fn eh_root(&self) -> bool {
            self.root
        }
        fn criar_dir_privado(&self) -> Result<PathBuf, String> {
            Ok(PathBuf::from("/privado"))
        }
        fn copiar(&self, de: &Path, para: &Path, limite: u64) -> Result<(), String> {
            let dados = self
                .arquivos
                .borrow()
                .get(de)
                .cloned()
                .ok_or("não existe")?;
            if dados.len() as u64 > limite {
                return Err("grande demais".into());
            }
            self.arquivos.borrow_mut().insert(para.to_path_buf(), dados);
            if let Some(novo) = &self.trocar_original_apos_copia {
                // ataque: troca o arquivo do usuário depois da cópia
                self.arquivos
                    .borrow_mut()
                    .insert(de.to_path_buf(), novo.clone());
            }
            Ok(())
        }
        fn ler(&self, c: &Path) -> Result<Vec<u8>, String> {
            self.arquivos
                .borrow()
                .get(c)
                .cloned()
                .ok_or("não existe".into())
        }
        fn formato(&self) -> Option<FormatoPacote> {
            self.formato
        }
        fn instalar(&self, _f: FormatoPacote, p: &Path) -> Result<(), String> {
            *self.instalado.borrow_mut() = Some(p.to_path_buf());
            Ok(())
        }
        fn remover_dir(&self, _c: &Path) {
            *self.removido.borrow_mut() = true;
        }
    }

    /// Manifesto + assinatura + pacote para a versão dada, assinados com uma chave de teste.
    fn cenario(versao: &str, pacote: &[u8]) -> (Falso, String, Vec<String>) {
        let par = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let manifesto = format!(
            r#"{{"versao":"{versao}","pacotes":{{"deb":{{"arquivo":"kwanto_{versao}_amd64.deb","sha256":"{}","tamanho":{}}}}}}}"#,
            sha256_hex(pacote),
            pacote.len()
        );
        let sig = minisign::sign(
            Some(&par.pk),
            &par.sk,
            Cursor::new(manifesto.as_bytes()),
            None,
            None,
        )
        .unwrap()
        .into_string();
        let mut arquivos = HashMap::new();
        arquivos.insert(PathBuf::from("/home/u/m.json"), manifesto.into_bytes());
        arquivos.insert(PathBuf::from("/home/u/m.json.minisig"), sig.into_bytes());
        arquivos.insert(
            PathBuf::from(format!("/home/u/kwanto_{versao}_amd64.deb")),
            pacote.to_vec(),
        );
        let sis = Falso {
            root: true,
            arquivos: RefCell::new(arquivos),
            formato: Some(FormatoPacote::Deb),
            instalado: RefCell::default(),
            trocar_original_apos_copia: None,
            removido: RefCell::default(),
        };
        let args = vec![
            "/home/u/m.json".into(),
            "/home/u/m.json.minisig".into(),
            format!("/home/u/kwanto_{versao}_amd64.deb"),
        ];
        (sis, par.pk.to_box().unwrap().into_string(), args)
    }
    fn v(t: &str) -> Versao {
        Versao::ler(t).unwrap()
    }

    #[test]
    fn caminho_feliz_instala_a_copia_privada_e_limpa() {
        let (sis, chave, args) = cenario("0.2.0", b"conteudo");
        assert_eq!(
            executar_ajudante(&args, &sis, &chave, v("0.1.0")).unwrap(),
            v("0.2.0")
        );
        assert_eq!(
            *sis.instalado.borrow(),
            Some(PathBuf::from("/privado/kwanto_0.2.0_amd64.deb"))
        );
        assert!(*sis.removido.borrow());
    }

    #[test]
    fn troca_do_arquivo_original_depois_da_copia_nao_afeta() {
        let (mut sis, chave, args) = cenario("0.2.0", b"conteudo");
        sis.trocar_original_apos_copia = Some(b"MALICIOSO".to_vec());
        assert!(executar_ajudante(&args, &sis, &chave, v("0.1.0")).is_ok());
        assert_eq!(
            sis.ler(Path::new("/privado/kwanto_0.2.0_amd64.deb"))
                .unwrap(),
            b"conteudo"
        );
    }

    #[test]
    fn recusa_rebaixamento_e_mesma_versao() {
        for instalada in ["0.2.0", "0.3.0"] {
            let (sis, chave, args) = cenario("0.2.0", b"conteudo");
            assert!(matches!(
                executar_ajudante(&args, &sis, &chave, v(instalada)),
                Err(ErroAtualizacao::Recusado(_))
            ));
            assert!(sis.instalado.borrow().is_none());
            assert!(*sis.removido.borrow(), "limpa mesmo quando recusa");
        }
    }

    #[test]
    fn recusa_pacote_adulterado() {
        let (sis, chave, args) = cenario("0.2.0", b"conteudo");
        sis.arquivos
            .borrow_mut()
            .insert(PathBuf::from(&args[2]), b"conteudX".to_vec());
        assert!(matches!(
            executar_ajudante(&args, &sis, &chave, v("0.1.0")),
            Err(ErroAtualizacao::Integridade(_))
        ));
        assert!(sis.instalado.borrow().is_none());
    }

    #[test]
    fn recusa_nome_de_arquivo_diferente_do_manifesto() {
        let (sis, chave, mut args) = cenario("0.2.0", b"conteudo");
        let dados = sis.ler(Path::new(&args[2])).unwrap();
        sis.arquivos
            .borrow_mut()
            .insert(PathBuf::from("/home/u/outro.deb"), dados);
        args[2] = "/home/u/outro.deb".into();
        assert!(matches!(
            executar_ajudante(&args, &sis, &chave, v("0.1.0")),
            Err(ErroAtualizacao::Recusado(_))
        ));
        assert!(sis.instalado.borrow().is_none());
    }

    #[test]
    fn recusa_formato_diferente_sem_root_e_argumentos_errados() {
        let (mut sis, chave, args) = cenario("0.2.0", b"conteudo");
        sis.formato = Some(FormatoPacote::Rpm);
        assert!(executar_ajudante(&args, &sis, &chave, v("0.1.0")).is_err());
        let (mut sis, chave, args) = cenario("0.2.0", b"conteudo");
        sis.formato = None;
        assert!(executar_ajudante(&args, &sis, &chave, v("0.1.0")).is_err());
        let (mut sis, chave, args) = cenario("0.2.0", b"conteudo");
        sis.root = false;
        assert!(executar_ajudante(&args, &sis, &chave, v("0.1.0")).is_err());
        let (sis, chave, _) = cenario("0.2.0", b"conteudo");
        assert!(executar_ajudante(&["só um".into()], &sis, &chave, v("0.1.0")).is_err());
        assert!(sis.instalado.borrow().is_none());
    }

    #[test]
    fn recusa_assinatura_de_outra_chave() {
        let (sis, _, args) = cenario("0.2.0", b"conteudo");
        let outra = minisign::KeyPair::generate_unencrypted_keypair()
            .unwrap()
            .pk
            .to_box()
            .unwrap()
            .into_string();
        assert!(matches!(
            executar_ajudante(&args, &sis, &outra, v("0.1.0")),
            Err(ErroAtualizacao::Assinatura)
        ));
        assert!(sis.instalado.borrow().is_none());
    }

    #[test]
    fn recusa_arquivo_grande_demais() {
        let (sis, chave, args) = cenario("0.2.0", b"conteudo");
        sis.arquivos
            .borrow_mut()
            .insert(PathBuf::from(&args[0]), vec![b' '; 2 * 1024 * 1024]);
        assert!(matches!(
            executar_ajudante(&args, &sis, &chave, v("0.1.0")),
            Err(ErroAtualizacao::Recusado(_))
        ));
    }
}
