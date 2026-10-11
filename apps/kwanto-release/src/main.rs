//! Ferramenta interna de release (não vai no pacote): cria a chave de assinatura, gera e assina o
//! manifesto. A chave privada fica só na máquina do mantenedor, cifrada por senha.

use anyhow::{Context, bail};
use kwanto_atualizacao::download::{conferir_pacote, sha256_hex};
use kwanto_atualizacao::manifesto::verificar_assinatura;
use kwanto_atualizacao::versao::Versao;
use minisign::{KeyPair, SecretKey};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

const USO: &str = "uso: kwanto-release <comando>
  criar-chave                       cria a chave oficial (pede a senha) em ~/.config/kwanto-release
  gerar-manifesto <versão> <dir>    escreve <dir>/manifesto.json com os pacotes de <dir>
  assinar <arquivo> [--chave <k>]   assina com a chave oficial (pede a senha)
  conferir <versão> <dir>           confere assinatura (chave do repositório), versão e pacotes
  conferir-binarios <arquivo>...    exige que cada binário confie só na chave do repositório
  criar-chave-teste <dir>           par SEM senha, só para testes locais
  assinar-teste <arquivo> <chave>   assina com uma chave de teste";

fn pasta_chave() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config/kwanto-release")
}

/// Manifesto com SHA-256 e tamanho do .deb e do .rpm da versão; falta de qualquer um é erro.
fn gerar_manifesto(versao: &str, dir: &Path) -> anyhow::Result<Vec<u8>> {
    let versao = Versao::ler(versao)?.to_string();
    let esperados = [
        ("deb", format!("kwanto_{versao}_amd64.deb")),
        ("rpm", format!("kwanto-{versao}-1.x86_64.rpm")),
    ];
    let mut pacotes = serde_json::Map::new();
    for (chave, arquivo) in esperados {
        let dados =
            std::fs::read(dir.join(&arquivo)).with_context(|| format!("faltou {arquivo}"))?;
        pacotes.insert(
            chave.into(),
            serde_json::json!({ "arquivo": arquivo, "sha256": sha256_hex(&dados), "tamanho": dados.len() }),
        );
    }
    Ok(serde_json::to_vec_pretty(
        &serde_json::json!({ "versao": versao, "pacotes": pacotes }),
    )?)
}

/// (chave secreta, chave pública), em texto no formato do minisign. `None` pede a senha.
fn novo_par(senha: Option<String>) -> anyhow::Result<(String, String)> {
    let par = KeyPair::generate_encrypted_keypair(senha)?;
    Ok((
        par.sk
            .to_box(Some("chave secreta do Kwanto"))?
            .into_string(),
        par.pk.to_box()?.into_string(),
    ))
}

fn novo_par_de_teste() -> anyhow::Result<(String, String)> {
    let par = KeyPair::generate_unencrypted_keypair()?;
    Ok((
        par.sk
            .to_box(Some("CHAVE DE TESTE do Kwanto (sem senha)"))?
            .into_string(),
        par.pk.to_box()?.into_string(),
    ))
}

/// `None` pede a senha no terminal.
fn abrir_chave(texto: &str, senha: Option<String>) -> anyhow::Result<SecretKey> {
    SecretKey::from_box(texto.to_string().into(), senha)
        .context("não foi possível abrir a chave (senha errada?)")
}

fn abrir_chave_de_teste(texto: &str) -> anyhow::Result<SecretKey> {
    Ok(SecretKey::from_unencrypted_box(texto.to_string().into())?)
}

fn assinar_bytes(sk: &SecretKey, dados: &[u8]) -> anyhow::Result<String> {
    Ok(minisign::sign(None, sk, std::io::Cursor::new(dados), None, None)?.into_string())
}

/// Confere o que vai ser publicado do jeito que as bandejas vão conferir: assinatura com a chave
/// pública do repositório, versão igual à da tag e cada pacote igual ao do manifesto.
fn conferir_publicacao(versao: &str, dir: &Path, chave_publica: &str) -> anyhow::Result<()> {
    let manifesto = std::fs::read(dir.join("manifesto.json")).context("faltou manifesto.json")?;
    let assinatura = std::fs::read_to_string(dir.join("manifesto.json.minisig"))
        .context("faltou manifesto.json.minisig")?;
    let m = verificar_assinatura(&manifesto, &assinatura, chave_publica)
        .context("a assinatura não confere com a chave pública do repositório")?;
    if m.versao()? != Versao::ler(versao)? {
        bail!("o manifesto é da versão {}, não da {versao}", m.versao);
    }
    for p in m.pacotes.values() {
        let dados =
            std::fs::read(dir.join(&p.arquivo)).with_context(|| format!("faltou {}", p.arquivo))?;
        conferir_pacote(&dados, p)?;
    }
    Ok(())
}

/// Chaves públicas minisign gravadas num binário: toda sequência "RW" + 54 caracteres base64 que
/// decodifica como chave Ed25519 do minisign, mesmo colada a outro texto (no binário, uma string
/// pode vir encostada na seguinte, sem separador).
fn chaves_embutidas(binario: &[u8]) -> Vec<String> {
    const TAMANHO: usize = 56;
    let mut chaves: Vec<String> = binario
        .windows(TAMANHO)
        .filter(|j| j.starts_with(b"RW"))
        .filter_map(|j| std::str::from_utf8(j).ok())
        .filter(|t| minisign::PublicKey::from_base64(t).is_ok())
        .map(str::to_string)
        .collect();
    chaves.sort();
    chaves.dedup();
    chaves
}

/// O binário publicado precisa confiar só na chave oficial (nunca numa chave de teste).
fn conferir_binario(binario: &[u8], chave_publica: &str) -> anyhow::Result<()> {
    let oficial = chave_publica
        .lines()
        .find(|l| l.starts_with("RW"))
        .context("chave pública sem a linha RW…")?
        .trim();
    let achadas = chaves_embutidas(binario);
    if achadas != [oficial] {
        bail!("o binário não confia só na chave oficial: encontrei {achadas:?}");
    }
    Ok(())
}

/// Grava sem sobrescrever; a chave secreta fica legível só pelo dono.
fn gravar_novo(caminho: &Path, conteudo: &str, modo: u32) -> anyhow::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(modo)
        .open(caminho)
        .with_context(|| format!("{} já existe ou não pode ser criado", caminho.display()))?
        .write_all(conteudo.as_bytes())?;
    Ok(())
}

fn criar_chave() -> anyhow::Result<()> {
    let pasta = pasta_chave();
    let (sk_path, pk_path) = (pasta.join("kwanto.key"), pasta.join("kwanto.pub"));
    if sk_path.exists() {
        bail!(
            "já existe uma chave em {}; não vou sobrescrever",
            sk_path.display()
        );
    }
    std::fs::create_dir_all(&pasta)?;
    let (sk, pk) = novo_par(None)?;
    gravar_novo(&sk_path, &sk, 0o600)?;
    gravar_novo(&pk_path, &pk, 0o644)?;
    std::fs::write("crates/kwanto-atualizacao/chave-publica.txt", &pk)
        .context("rode a partir da raiz do repositório")?;
    println!(
        "Chave criada em {}.\nFAÇA BACKUP de {} (pendrive ou gerenciador de senhas) e guarde a senha.\n\
         A chave pública foi copiada para crates/kwanto-atualizacao/chave-publica.txt; faça commit dela.",
        pasta.display(),
        sk_path.display()
    );
    Ok(())
}

fn criar_chave_teste(dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    let (sk, pk) = novo_par_de_teste()?;
    gravar_novo(&dir.join("teste.key"), &sk, 0o600)?;
    gravar_novo(&dir.join("teste.pub"), &pk, 0o644)?;
    println!("chave de teste em {}", dir.display());
    Ok(())
}

fn gravar_assinatura(arquivo: &Path, sk: &SecretKey) -> anyhow::Result<()> {
    let assinatura = assinar_bytes(sk, &std::fs::read(arquivo)?)?;
    let saida = PathBuf::from(format!("{}.minisig", arquivo.display()));
    std::fs::write(&saida, assinatura)?;
    println!("assinado: {}", saida.display());
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["criar-chave"] => criar_chave(),
        ["criar-chave-teste", dir] => criar_chave_teste(Path::new(dir)),
        ["gerar-manifesto", versao, dir] => {
            let m = gerar_manifesto(versao, Path::new(dir))?;
            std::fs::write(Path::new(dir).join("manifesto.json"), m)?;
            Ok(())
        }
        ["conferir", versao, dir] => {
            let chave = std::fs::read_to_string("crates/kwanto-atualizacao/chave-publica.txt")
                .context("rode a partir da raiz do repositório")?;
            conferir_publicacao(versao, Path::new(dir), &chave)?;
            println!("conferido: {dir} está pronto para publicar a versão {versao}");
            Ok(())
        }
        ["conferir-binarios", ref arquivos @ ..] if !arquivos.is_empty() => {
            let chave = std::fs::read_to_string("crates/kwanto-atualizacao/chave-publica.txt")
                .context("rode a partir da raiz do repositório")?;
            for a in arquivos {
                conferir_binario(&std::fs::read(a).with_context(|| a.to_string())?, &chave)
                    .with_context(|| a.to_string())?;
                println!("ok: {a} confia só na chave oficial");
            }
            Ok(())
        }
        ["assinar", arquivo] => {
            let texto = std::fs::read_to_string(pasta_chave().join("kwanto.key"))?;
            gravar_assinatura(Path::new(arquivo), &abrir_chave(&texto, None)?)
        }
        ["assinar", arquivo, "--chave", chave] => {
            let texto = std::fs::read_to_string(chave)?;
            gravar_assinatura(Path::new(arquivo), &abrir_chave(&texto, None)?)
        }
        ["assinar-teste", arquivo, chave] => {
            let texto = std::fs::read_to_string(chave)?;
            gravar_assinatura(Path::new(arquivo), &abrir_chave_de_teste(&texto)?)
        }
        _ => bail!("{USO}"),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn pacotes_falsos(dir: &Path) {
        std::fs::write(dir.join("kwanto_0.2.0_amd64.deb"), b"deb").unwrap();
        std::fs::write(dir.join("kwanto-0.2.0-1.x86_64.rpm"), b"rpm!").unwrap();
        std::fs::write(dir.join("outro.txt"), b"x").unwrap();
    }

    #[test]
    fn manifesto_gerado_lista_os_pacotes_com_hash_e_tamanho() {
        let d = tempfile::tempdir().unwrap();
        pacotes_falsos(d.path());
        let m = gerar_manifesto("0.2.0", d.path()).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&m).unwrap();
        assert_eq!(v["versao"], "0.2.0");
        assert_eq!(v["pacotes"]["deb"]["arquivo"], "kwanto_0.2.0_amd64.deb");
        assert_eq!(v["pacotes"]["deb"]["tamanho"], 3);
        assert_eq!(
            v["pacotes"]["rpm"]["sha256"],
            kwanto_atualizacao::download::sha256_hex(b"rpm!")
        );
        assert!(v["pacotes"].get("txt").is_none());
    }

    #[test]
    fn falta_pacote_ou_versao_invalida_e_erro() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("kwanto_0.2.0_amd64.deb"), b"deb").unwrap();
        assert!(
            gerar_manifesto("0.2.0", d.path()).is_err(),
            "sem .rpm não publica"
        );
        pacotes_falsos(d.path());
        assert!(gerar_manifesto("0.2", d.path()).is_err());
    }

    #[test]
    fn chave_com_senha_assina_e_o_app_aceita() {
        let d = tempfile::tempdir().unwrap();
        pacotes_falsos(d.path());
        let m = gerar_manifesto("0.2.0", d.path()).unwrap();
        let (sk_txt, pk_txt) = novo_par(Some("senha forte".into())).unwrap();
        let sk = abrir_chave(&sk_txt, Some("senha forte".into())).unwrap();
        let assinatura = assinar_bytes(&sk, &m).unwrap();
        let lido = verificar_assinatura(&m, &assinatura, &pk_txt).unwrap();
        assert_eq!(lido.versao, "0.2.0");
        assert!(abrir_chave(&sk_txt, Some("senha errada".into())).is_err());
    }

    /// Pacotes, manifesto e assinatura prontos para publicar, com uma chave de teste.
    fn pronto_para_publicar() -> (tempfile::TempDir, String) {
        let d = tempfile::tempdir().unwrap();
        pacotes_falsos(d.path());
        let m = gerar_manifesto("0.2.0", d.path()).unwrap();
        std::fs::write(d.path().join("manifesto.json"), &m).unwrap();
        let (sk_txt, pk_txt) = novo_par_de_teste().unwrap();
        let sk = abrir_chave_de_teste(&sk_txt).unwrap();
        std::fs::write(
            d.path().join("manifesto.json.minisig"),
            assinar_bytes(&sk, &m).unwrap(),
        )
        .unwrap();
        (d, pk_txt)
    }

    #[test]
    fn conferencia_aceita_o_que_as_bandejas_vao_aceitar() {
        let (d, pk) = pronto_para_publicar();
        conferir_publicacao("0.2.0", d.path(), &pk).unwrap();
        conferir_publicacao("v0.2.0", d.path(), &pk).unwrap();
    }

    #[test]
    fn conferencia_recusa_outra_chave_outra_versao_e_pacote_trocado() {
        let (d, _) = pronto_para_publicar();
        let (_, outra) = novo_par_de_teste().unwrap();
        assert!(
            conferir_publicacao("0.2.0", d.path(), &outra).is_err(),
            "chave diferente da do repositório"
        );

        let (d, pk) = pronto_para_publicar();
        assert!(
            conferir_publicacao("0.2.1", d.path(), &pk).is_err(),
            "versão diferente da tag"
        );

        let (d, pk) = pronto_para_publicar();
        std::fs::write(d.path().join("kwanto-0.2.0-1.x86_64.rpm"), b"rpm?").unwrap();
        assert!(
            conferir_publicacao("0.2.0", d.path(), &pk).is_err(),
            "pacote trocado depois do manifesto"
        );

        let (d, pk) = pronto_para_publicar();
        std::fs::remove_file(d.path().join("manifesto.json.minisig")).unwrap();
        assert!(
            conferir_publicacao("0.2.0", d.path(), &pk).is_err(),
            "sem assinatura"
        );
    }

    #[test]
    fn binario_precisa_ter_so_a_chave_oficial() {
        let (_, oficial) = novo_par_de_teste().unwrap();
        let (_, teste) = novo_par_de_teste().unwrap();
        let base64 = |pk: &str| pk.lines().nth(1).unwrap().to_string();
        let binario = |chaves: &[&str]| {
            let mut b = b"\x7fELF lixo binario ".to_vec();
            for c in chaves {
                b.extend_from_slice(c.as_bytes());
                b.extend_from_slice(b"\0mais lixo");
            }
            b
        };
        assert_eq!(
            chaves_embutidas(&binario(&[&oficial])),
            vec![base64(&oficial)]
        );
        assert!(conferir_binario(&binario(&[&oficial]), &oficial).is_ok());
        assert!(
            conferir_binario(&binario(&[&teste]), &oficial).is_err(),
            "chave de teste embutida"
        );
        assert!(
            conferir_binario(&binario(&[&oficial, &teste]), &oficial).is_err(),
            "duas chaves"
        );
        assert!(
            conferir_binario(&binario(&[]), &oficial).is_err(),
            "sem chave"
        );
        // Chave colada no texto seguinte, sem separador (como sai de uma variável sem \n final).
        let mut colado = binario(&[&oficial]);
        colado.extend_from_slice(b"untrusted comment: x\n");
        colado.extend_from_slice(base64(&teste).as_bytes());
        colado.extend_from_slice(b"untrusted comment: minisign");
        assert!(
            conferir_binario(&colado, &oficial).is_err(),
            "chave de teste colada em outro texto"
        );
    }

    #[test]
    fn chave_de_teste_sem_senha() {
        let (sk_txt, pk_txt) = novo_par_de_teste().unwrap();
        let sk = abrir_chave_de_teste(&sk_txt).unwrap();
        let m = br#"{"versao":"0.0.2","pacotes":{}}"#;
        assert!(verificar_assinatura(m, &assinar_bytes(&sk, m).unwrap(), &pk_txt).is_ok());
    }
}
