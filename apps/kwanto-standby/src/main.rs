//! Consulta, a cada 30 s, se cada HDD está em standby e grava o resultado para o `kwantod`. É o
//! único processo do Kwanto com acesso aos discos (grupo `disk` e `CAP_SYS_RAWIO`); não tem rede
//! nem lê nada que venha de fora (ver `packaging/kwanto-standby.service`).
use kwanto_sensors::discos::StandbyAta;
use kwanto_sensors::raiz::RaizDoSistema;
use kwanto_sensors::standby::{CAMINHO_ESTADO, consultar, gravar};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const INTERVALO: Duration = Duration::from_secs(30);

fn main() {
    let raiz = RaizDoSistema::real();
    loop {
        let agora = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|d| i64::try_from(d.as_secs()).ok())
            .unwrap_or(0);
        let estado = consultar(&raiz, &StandbyAta, agora);
        if let Err(e) = gravar(&estado, Path::new(CAMINHO_ESTADO)) {
            eprintln!("não foi possível gravar {CAMINHO_ESTADO}: {e}");
        }
        std::thread::sleep(INTERVALO);
    }
}
