//! Ajudante root do atualizador, chamado pela bandeja via pkexec. Toda a lógica e as checagens
//! estão em `kwanto_atualizacao::ajudante`; aqui só o acesso real ao sistema.

use kwanto_atualizacao::ajudante::{CAMINHO_AJUDANTE, SistemaAjudante, executar_ajudante};
use kwanto_atualizacao::formato::{ExecutorReal, FormatoPacote, detectar_formato};
use kwanto_atualizacao::versao::Versao;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BASE_PRIVADA: &str = "/var/cache/kwanto-atualizar";
/// Quanto esperar, em segundos, se outro gerenciador de pacotes (ex.: atualizações automáticas)
/// estiver usando o sistema.
const ESPERA_TRAVA: &str = "300";

/// Instala o pacote local sem perguntar nada: o pkexec limpa o ambiente e não há terminal.
/// No deb, mantém arquivos de configuração editados pelo usuário e nunca remove outros pacotes.
fn comando_instalacao(formato: FormatoPacote, pacote: &Path, tem_dnf: bool) -> Command {
    let mut c;
    match formato {
        FormatoPacote::Deb => {
            c = Command::new("apt-get");
            c.args(["-o", &format!("DPkg::Lock::Timeout={ESPERA_TRAVA}")])
                .args(["-o", "Dpkg::Options::=--force-confdef"])
                .args(["-o", "Dpkg::Options::=--force-confold"])
                .args(["install", "-y", "--no-remove"])
                .env("DEBIAN_FRONTEND", "noninteractive");
        }
        FormatoPacote::Rpm if tem_dnf => {
            // O dnf já espera a trava sozinho.
            c = Command::new("dnf");
            c.args(["install", "-y"]);
        }
        FormatoPacote::Rpm => {
            c = Command::new("zypper");
            c.args(["--non-interactive", "install", "--allow-unsigned-rpm"])
                .env("ZYPP_LOCK_TIMEOUT", ESPERA_TRAVA);
        }
    }
    c.arg(pacote);
    c
}

#[derive(Debug)]
struct SistemaReal;

impl SistemaAjudante for SistemaReal {
    fn eh_root(&self) -> bool {
        rustix::process::geteuid().is_root()
    }

    fn criar_dir_privado(&self) -> Result<PathBuf, String> {
        std::fs::create_dir_all(BASE_PRIVADA).map_err(|e| e.to_string())?;
        // Nome aleatório e permissão 0700.
        let dir = tempfile::Builder::new()
            .prefix("atualizacao-")
            .tempdir_in(BASE_PRIVADA)
            .map_err(|e| e.to_string())?;
        Ok(dir.keep())
    }

    /// Só arquivo comum: sem seguir link simbólico e sem travar em FIFO ou dispositivo.
    fn copiar(&self, de: &Path, para: &Path, limite: u64) -> Result<(), String> {
        let flags = rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK;
        let origem = OpenOptions::new()
            .read(true)
            .custom_flags(i32::try_from(flags.bits()).unwrap_or_default())
            .open(de)
            .map_err(|e| format!("{}: {e}", de.display()))?;
        let meta = origem.metadata().map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err(format!("{} não é um arquivo comum", de.display()));
        }
        if meta.len() > limite {
            return Err(format!("{} é grande demais", de.display()));
        }
        let mut dados = Vec::new();
        origem
            .take(limite.saturating_add(1))
            .read_to_end(&mut dados)
            .map_err(|e| e.to_string())?;
        if dados.len() as u64 > limite {
            return Err(format!("{} é grande demais", de.display()));
        }
        let mut destino = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(para)
            .map_err(|e| format!("{}: {e}", para.display()))?;
        destino.write_all(&dados).map_err(|e| e.to_string())
    }

    fn ler(&self, caminho: &Path) -> Result<Vec<u8>, String> {
        std::fs::read(caminho).map_err(|e| e.to_string())
    }

    fn formato(&self) -> Option<FormatoPacote> {
        detectar_formato(&ExecutorReal, CAMINHO_AJUDANTE)
    }

    fn instalar(&self, formato: FormatoPacote, pacote: &Path) -> Result<(), String> {
        let status = comando_instalacao(formato, pacote, Path::new("/usr/bin/dnf").exists())
            .stdin(Stdio::null())
            .status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(format!("o gerenciador de pacotes terminou com {s}")),
            Err(e) => Err(e.to_string()),
        }
    }

    fn remover_dir(&self, caminho: &Path) {
        let _ = std::fs::remove_dir_all(caminho);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match executar_ajudante(
        &args,
        &SistemaReal,
        kwanto_atualizacao::chave_oficial(),
        Versao::atual(),
    ) {
        Ok(v) => println!("Kwanto atualizado para {v}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::path::Path;

    fn partes(c: &Command) -> (String, Vec<String>, Vec<(String, String)>) {
        (
            c.get_program().to_string_lossy().into_owned(),
            c.get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect(),
            c.get_envs()
                .filter_map(|(k, v)| {
                    Some((
                        k.to_string_lossy().into_owned(),
                        v?.to_string_lossy().into_owned(),
                    ))
                })
                .collect(),
        )
    }

    #[test]
    fn apt_espera_a_trava_mantem_configuracoes_e_nao_pergunta() {
        let (programa, args, env) = partes(&comando_instalacao(
            FormatoPacote::Deb,
            Path::new("/p/k.deb"),
            true,
        ));
        assert_eq!(programa, "apt-get");
        assert_eq!(
            args,
            [
                "-o",
                "DPkg::Lock::Timeout=300",
                "-o",
                "Dpkg::Options::=--force-confdef",
                "-o",
                "Dpkg::Options::=--force-confold",
                "install",
                "-y",
                "--no-remove",
                "/p/k.deb",
            ]
        );
        assert!(env.contains(&("DEBIAN_FRONTEND".into(), "noninteractive".into())));
    }

    #[test]
    fn rpm_usa_dnf_ou_zypper_esperando_a_trava() {
        let (programa, args, _) = partes(&comando_instalacao(
            FormatoPacote::Rpm,
            Path::new("/p/k.rpm"),
            true,
        ));
        assert_eq!(programa, "dnf");
        assert_eq!(args, ["install", "-y", "/p/k.rpm"]);
        let (programa, args, env) = partes(&comando_instalacao(
            FormatoPacote::Rpm,
            Path::new("/p/k.rpm"),
            false,
        ));
        assert_eq!(programa, "zypper");
        assert_eq!(
            args,
            [
                "--non-interactive",
                "install",
                "--allow-unsigned-rpm",
                "/p/k.rpm"
            ]
        );
        assert!(env.contains(&("ZYPP_LOCK_TIMEOUT".into(), "300".into())));
    }

    #[test]
    fn copia_arquivo_comum_dentro_do_limite() {
        let d = tempfile::tempdir().unwrap();
        let de = d.path().join("a");
        std::fs::write(&de, b"12345").unwrap();
        let para = d.path().join("b");
        SistemaReal.copiar(&de, &para, 5).unwrap();
        assert_eq!(std::fs::read(&para).unwrap(), b"12345");
        assert!(SistemaReal.copiar(&de, &d.path().join("c"), 4).is_err());
    }

    #[test]
    fn recusa_link_simbolico_diretorio_e_fifo() {
        let d = tempfile::tempdir().unwrap();
        let alvo = d.path().join("alvo");
        std::fs::write(&alvo, b"segredo").unwrap();
        let link = d.path().join("link");
        std::os::unix::fs::symlink(&alvo, &link).unwrap();
        assert!(SistemaReal.copiar(&link, &d.path().join("x"), 100).is_err());
        assert!(
            SistemaReal
                .copiar(d.path(), &d.path().join("y"), 100)
                .is_err()
        );
        let fifo = d.path().join("fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        // Não pode travar esperando alguém escrever no FIFO.
        assert!(SistemaReal.copiar(&fifo, &d.path().join("z"), 100).is_err());
        assert!(!Path::new(&d.path().join("z")).exists());
    }
}
