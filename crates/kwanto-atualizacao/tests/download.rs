use kwanto_atualizacao::download::{Origem, baixar, conferir_pacote, sha256_hex};
use kwanto_atualizacao::manifesto::Pacote;
use kwanto_atualizacao::versao::Versao;
use std::io::{Read, Write};
use std::net::TcpListener;

fn servidor(corpo: Vec<u8>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut c in l.incoming().flatten() {
            let _ = c.read(&mut [0u8; 1024]);
            let _ = write!(
                c,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                corpo.len()
            );
            let _ = c.write_all(&corpo);
        }
    });
    url
}

#[test]
fn urls_oficiais_e_locais() {
    let v = Versao::ler("0.2.0").unwrap();
    let o = Origem::Oficial;
    assert_eq!(
        o.url_manifesto(),
        "https://github.com/gabrielpcruz/kwanto/releases/latest/download/manifesto.json"
    );
    assert_eq!(
        o.url_assinatura(),
        "https://github.com/gabrielpcruz/kwanto/releases/latest/download/manifesto.json.minisig"
    );
    assert_eq!(
        o.url_pacote(&v, "kwanto_0.2.0_amd64.deb"),
        "https://github.com/gabrielpcruz/kwanto/releases/download/v0.2.0/kwanto_0.2.0_amd64.deb"
    );
    let l = Origem::Local("http://127.0.0.1:9000/".into());
    assert_eq!(l.url_manifesto(), "http://127.0.0.1:9000/manifesto.json");
    assert_eq!(l.url_pacote(&v, "x.deb"), "http://127.0.0.1:9000/x.deb");
}

#[test]
fn baixa_e_respeita_o_limite() {
    let url = servidor(vec![7u8; 1000]);
    assert_eq!(baixar(&url, 5000).unwrap().len(), 1000);
    assert!(baixar(&url, 999).is_err(), "maior que o limite deve falhar");
    assert!(baixar("http://127.0.0.1:1/x", 10).is_err());
}

#[test]
fn baixa_mais_que_o_limite_padrao_do_ureq() {
    // O ureq limita corpos a 10 MB por padrão; pacotes podem chegar a 200 MB.
    let url = servidor(vec![1u8; 11 * 1024 * 1024]);
    assert_eq!(
        baixar(&url, 20 * 1024 * 1024).unwrap().len(),
        11 * 1024 * 1024
    );
}

#[test]
fn integridade_por_sha256_e_tamanho() {
    let dados = b"pacote de teste";
    let p = Pacote {
        arquivo: "k.deb".into(),
        sha256: sha256_hex(dados),
        tamanho: dados.len() as u64,
    };
    assert!(conferir_pacote(dados, &p).is_ok());
    assert!(conferir_pacote(b"pacote de testE", &p).is_err());
    let p2 = Pacote { tamanho: 99, ..p };
    assert!(conferir_pacote(dados, &p2).is_err());
}
