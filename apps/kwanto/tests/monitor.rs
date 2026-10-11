use async_trait::async_trait;
use kwanto::aparencia::Aparencia;
use kwanto::monitor::{
    Bandeja, Resposta, Servico, UltimoStatus, acompanhar, buscar_status, uid_no_passwd,
};
use kwanto::{Avisar, Evento};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::MetadataExt as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Servidor HTTP mínimo: responde /api/status enquanto `no_ar` for verdadeiro.
fn servidor(no_ar: Arc<AtomicBool>) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", l.local_addr().unwrap());
    std::thread::spawn(move || {
        for conexao in l.incoming().flatten() {
            let mut c = conexao;
            let mut buf = [0u8; 1024];
            let _ = c.read(&mut buf);
            if no_ar.load(Ordering::SeqCst) {
                let corpo = r#"{"agora":{"parede_w":99.4,"custo_hora":0.08},"hoje":{"custo":1},"ciclo":{"previsao_custo":50}}"#;
                let _ = write!(
                    c,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{corpo}",
                    corpo.len()
                );
            } else {
                let _ = c.write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        }
    });
    url
}

#[derive(Default)]
struct BandejaFalsa(Mutex<Vec<Aparencia>>);
#[async_trait]
impl Bandeja for BandejaFalsa {
    async fn mostrar(&self, a: Aparencia) {
        self.0.lock().unwrap().push(a);
    }
}

fn meu_uid() -> u32 {
    std::fs::metadata("/proc/self").unwrap().uid()
}

fn servico(url: &str, donos: Vec<u32>) -> Servico {
    Servico::da_url(url, donos).unwrap()
}

#[test]
fn buscar_status_com_servico_no_ar_e_fora() {
    let no_ar = Arc::new(AtomicBool::new(true));
    let url = servidor(no_ar.clone());
    let s = servico(&url, vec![meu_uid()]);
    let Resposta::NoAr(v) = buscar_status(&s) else {
        panic!("devia estar no ar")
    };
    assert_eq!(v["agora"]["parede_w"], 99.4);
    no_ar.store(false, Ordering::SeqCst);
    assert_eq!(buscar_status(&s), Resposta::Fora);
    assert_eq!(
        buscar_status(&servico("http://127.0.0.1:1", vec![meu_uid()])),
        Resposta::Fora,
        "nada escutando"
    );
}

/// Outro usuário ocupando a porta (com o serviço parado): a bandeja não usa nada do que ele responde.
#[test]
fn servidor_de_outro_dono_nao_e_confiavel() {
    let url = servidor(Arc::new(AtomicBool::new(true)));
    let s = servico(&url, vec![4242]);
    assert_eq!(buscar_status(&s), Resposta::DonoEstranho(meu_uid()));
}

#[test]
fn so_enderecos_locais_em_http() {
    assert!(Servico::da_url("http://127.0.0.1:8787", vec![1]).is_some());
    assert!(Servico::da_url("http://localhost:8787/", vec![1]).is_some());
    for url in [
        "https://127.0.0.1:8787",
        "http://192.168.0.10:8787",
        "http://exemplo.com:8787",
        "http://127.0.0.1",
        "lixo",
    ] {
        assert!(Servico::da_url(url, vec![1]).is_none(), "{url}");
    }
}

#[test]
fn acha_o_uid_do_usuario_do_servico_no_passwd() {
    let passwd = "root:x:0:0:root:/root:/bin/bash\nkwanto-standby:x:998:998::/:/usr/sbin/nologin\nkwanto:x:999:999:Kwanto:/var/lib/kwanto:/usr/sbin/nologin\n";
    assert_eq!(uid_no_passwd(passwd, "kwanto"), Some(999));
    assert_eq!(uid_no_passwd(passwd, "ninguem"), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn acompanhar_atualiza_o_icone_e_avisa_so_nas_transicoes() {
    let no_ar = Arc::new(AtomicBool::new(false));
    let url = servidor(no_ar.clone());
    let bandeja = Arc::new(BandejaFalsa::default());
    let eventos = Arc::new(Mutex::new(Vec::new()));
    let e = eventos.clone();
    let avisar: Avisar = Arc::new(move |ev| e.lock().unwrap().push(ev));
    let ultimo = UltimoStatus::default();
    tokio::spawn(acompanhar(
        Some(servico(&url, vec![meu_uid()])),
        ultimo.clone(),
        bandeja.clone(),
        avisar,
        Duration::from_millis(100),
    ));

    tokio::time::sleep(Duration::from_millis(350)).await; // fora do ar
    assert!(ultimo.lock().unwrap().is_none(), "nada para o popup");
    no_ar.store(true, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(350)).await; // voltou

    let json: serde_json::Value =
        serde_json::from_str(ultimo.lock().unwrap().as_deref().unwrap()).unwrap();
    assert_eq!(
        json["agora"]["parede_w"], 99.4,
        "o popup recebe o status verificado"
    );
    let aparencias = bandeja.0.lock().unwrap().clone();
    assert_eq!(aparencias.first().unwrap().rotulo, "– W");
    assert_eq!(aparencias.last().unwrap().rotulo, "99 W");
    let ev = eventos.lock().unwrap().clone();
    assert_eq!(
        ev,
        vec![Evento::Servico(false), Evento::Servico(true)],
        "deve avisar só quando muda"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn com_dono_estranho_a_bandeja_fica_como_parada() {
    let url = servidor(Arc::new(AtomicBool::new(true)));
    let bandeja = Arc::new(BandejaFalsa::default());
    let eventos = Arc::new(Mutex::new(Vec::new()));
    let e = eventos.clone();
    let avisar: Avisar = Arc::new(move |ev| e.lock().unwrap().push(ev));
    let ultimo = UltimoStatus::default();
    tokio::spawn(acompanhar(
        Some(servico(&url, vec![4242])),
        ultimo.clone(),
        bandeja.clone(),
        avisar,
        Duration::from_millis(100),
    ));
    tokio::time::sleep(Duration::from_millis(350)).await;
    assert!(ultimo.lock().unwrap().is_none());
    assert_eq!(bandeja.0.lock().unwrap().last().unwrap().rotulo, "– W");
    assert_eq!(
        eventos.lock().unwrap().clone(),
        vec![Evento::Servico(false)]
    );
}
