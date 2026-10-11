use kwanto::atualizador::{Oferta, preparar, verificar};
use kwanto_atualizacao::ErroAtualizacao;
use kwanto_atualizacao::download::{Origem, sha256_hex};
use kwanto_atualizacao::formato::FormatoPacote;
use kwanto_atualizacao::manifesto::{Manifesto, Pacote};
use kwanto_atualizacao::versao::Versao;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;

/// Servidor HTTP mínimo que entrega arquivos pelo caminho.
fn servidor(arquivos: HashMap<String, Vec<u8>>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut c in l.incoming().flatten() {
            let mut buf = [0u8; 2048];
            let n = c.read(&mut buf).unwrap_or(0);
            let pedido = String::from_utf8_lossy(&buf[..n]).to_string();
            let caminho = pedido.split_whitespace().nth(1).unwrap_or("/").to_string();
            match arquivos.get(&caminho) {
                Some(corpo) => {
                    let _ = write!(
                        c,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        corpo.len()
                    );
                    let _ = c.write_all(corpo);
                }
                None => {
                    let _ = write!(
                        c,
                        "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    );
                }
            }
        }
    });
    url
}

fn oferta(pacote: &[u8]) -> Oferta {
    let mut pacotes = std::collections::BTreeMap::new();
    pacotes.insert(
        "deb".to_string(),
        Pacote {
            arquivo: "kwanto_9.0.0_amd64.deb".into(),
            sha256: sha256_hex(pacote),
            tamanho: pacote.len() as u64,
        },
    );
    Oferta {
        manifesto: Manifesto {
            versao: "9.0.0".into(),
            pacotes,
        },
        bytes: b"{manifesto}".to_vec(),
        assinatura: "assinatura".into(),
        versao: Versao::ler("9.0.0").unwrap(),
    }
}

#[test]
fn manifesto_assinado_por_outra_chave_nao_vira_oferta() {
    let manifesto = br#"{"versao":"9.0.0","pacotes":{}}"#.to_vec();
    let par = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let sig = minisign::sign(None, &par.sk, std::io::Cursor::new(&manifesto), None, None)
        .unwrap()
        .into_string();
    let url = servidor(HashMap::from([
        ("/manifesto.json".to_string(), manifesto),
        ("/manifesto.json.minisig".to_string(), sig.into_bytes()),
    ]));
    assert!(matches!(
        verificar(&Origem::Local(url)),
        Err(ErroAtualizacao::Assinatura)
    ));
}

#[test]
fn sem_assinatura_publicada_nao_vira_oferta() {
    let url = servidor(HashMap::from([(
        "/manifesto.json".to_string(),
        br#"{"versao":"9.0.0","pacotes":{}}"#.to_vec(),
    )]));
    assert!(verificar(&Origem::Local(url)).is_err());
}

#[test]
fn preparar_baixa_confere_e_grava_os_tres_arquivos() {
    let url = servidor(HashMap::from([(
        "/kwanto_9.0.0_amd64.deb".to_string(),
        b"pacote".to_vec(),
    )]));
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("atualizacao");
    let [m, a, p] = preparar(
        &Origem::Local(url),
        &oferta(b"pacote"),
        FormatoPacote::Deb,
        &dir,
    )
    .unwrap();
    assert_eq!(std::fs::read(m).unwrap(), b"{manifesto}");
    assert_eq!(std::fs::read(a).unwrap(), b"assinatura");
    assert!(p.ends_with("kwanto_9.0.0_amd64.deb"));
    assert_eq!(std::fs::read(p).unwrap(), b"pacote");
}

#[test]
fn preparar_recusa_pacote_adulterado_sem_gravar_nada() {
    let url = servidor(HashMap::from([(
        "/kwanto_9.0.0_amd64.deb".to_string(),
        b"pacotX".to_vec(),
    )]));
    let d = tempfile::tempdir().unwrap();
    let dir = d.path().join("atualizacao");
    let r = preparar(
        &Origem::Local(url),
        &oferta(b"pacote"),
        FormatoPacote::Deb,
        &dir,
    );
    assert!(matches!(r, Err(ErroAtualizacao::Integridade(_))));
    assert!(!dir.exists());
}

#[test]
fn preparar_sem_pacote_do_formato_e_recusado() {
    let d = tempfile::tempdir().unwrap();
    let r = preparar(
        &Origem::Local("http://127.0.0.1:1".into()),
        &oferta(b"x"),
        FormatoPacote::Rpm,
        d.path(),
    );
    assert!(matches!(r, Err(ErroAtualizacao::Recusado(_))));
}
