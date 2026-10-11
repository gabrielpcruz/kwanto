//! Barramento D-Bus privado para os testes (não mexe na bandeja real do usuário).
#![allow(dead_code)]
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub struct BarramentoPrivado {
    processo: Child,
    pub endereco: String,
}

impl BarramentoPrivado {
    pub fn novo() -> Self {
        let mut processo = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon");
        let mut linha = String::new();
        BufReader::new(processo.stdout.take().unwrap())
            .read_line(&mut linha)
            .unwrap();
        Self {
            processo,
            endereco: linha.trim().to_string(),
        }
    }

    pub async fn conectar(&self) -> zbus::Connection {
        zbus::connection::Builder::address(self.endereco.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}

impl Drop for BarramentoPrivado {
    fn drop(&mut self) {
        let _ = self.processo.kill();
    }
}

/// Imita a bandeja do ambiente gráfico: guarda quem se registrou.
#[derive(Clone, Default)]
pub struct WatcherFalso(pub Arc<Mutex<Vec<String>>>);

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl WatcherFalso {
    fn register_status_notifier_item(&self, servico: String) {
        self.0.lock().unwrap().push(servico);
    }
}

pub async fn subir_watcher(conn: &zbus::Connection) -> WatcherFalso {
    let w = WatcherFalso::default();
    conn.object_server()
        .at("/StatusNotifierWatcher", w.clone())
        .await
        .unwrap();
    conn.request_name("org.kde.StatusNotifierWatcher")
        .await
        .unwrap();
    w
}

/// Espera até `cond` ficar verdadeira (máx. 3 s).
pub async fn esperar(mut cond: impl FnMut() -> bool) -> bool {
    for _ in 0..60 {
        if cond() {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    false
}
