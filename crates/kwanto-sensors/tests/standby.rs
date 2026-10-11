//! Standby dos HDDs fora do serviço principal: o `kwanto-standby` (único com o privilégio de
//! consultar o disco) grava um arquivo, e o `kwantod` só o lê.
use kwanto_sensors::discos::ConsultaStandby;
use kwanto_sensors::raiz::RaizDoSistema;
use kwanto_sensors::standby::{EstadoStandby, StandbyArquivo, consultar, gravar, hdds};
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn escrever(base: &Path, rel: &str, conteudo: &str) {
    let p = base.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, conteudo).unwrap();
}

fn disco(base: &Path, dev: &str, rot: &str) {
    escrever(base, &format!("sys/block/{dev}/size"), "1000");
    escrever(base, &format!("sys/block/{dev}/queue/rotational"), rot);
}

fn agora() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

struct Fixo;
impl ConsultaStandby for Fixo {
    fn em_standby(&self, d: &Path) -> Option<bool> {
        match d.file_name()?.to_str()? {
            "sda" => Some(true),
            "sdb" => Some(false),
            _ => None,
        }
    }
}

#[test]
fn so_hdds_sao_consultados_e_o_estado_e_gravado_com_seguranca() {
    let t = tempfile::tempdir().unwrap();
    disco(t.path(), "sda", "1");
    disco(t.path(), "sdb", "1");
    disco(t.path(), "sdc", "1"); // não responde
    disco(t.path(), "sdd", "0"); // SSD
    disco(t.path(), "nvme0n1", "0");
    let raiz = RaizDoSistema::em(t.path().to_path_buf());
    let lista: Vec<String> = hdds(&raiz)
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(lista, ["sda", "sdb", "sdc"]);

    let estado = consultar(&raiz, &Fixo, 1_000);
    assert_eq!(estado.atualizado, 1_000);
    assert_eq!(estado.discos.get("sda"), Some(&true));
    assert_eq!(estado.discos.get("sdb"), Some(&false));
    assert_eq!(estado.discos.get("sdc"), None, "sem resposta fica de fora");

    let destino = t.path().join("run/estado.json");
    fs::create_dir_all(destino.parent().unwrap()).unwrap();
    gravar(&estado, &destino).unwrap();
    assert_eq!(
        fs::metadata(&destino).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(
        fs::read_dir(destino.parent().unwrap()).unwrap().count(),
        1,
        "sem arquivo temporário sobrando"
    );
}

#[test]
fn kwantod_le_o_arquivo_e_desconfia_de_estado_velho_ou_corrompido() {
    let t = tempfile::tempdir().unwrap();
    let caminho = t.path().join("estado.json");
    let leitor = StandbyArquivo::novo(&caminho);
    let dev = |n: &str| t.path().join("dev").join(n);
    assert_eq!(leitor.em_standby(&dev("sda")), None, "sem arquivo");

    let mut estado = EstadoStandby {
        atualizado: agora(),
        discos: Default::default(),
    };
    estado.discos.insert("sda".into(), true);
    estado.discos.insert("sdb".into(), false);
    gravar(&estado, &caminho).unwrap();
    assert_eq!(leitor.em_standby(&dev("sda")), Some(true));
    assert_eq!(leitor.em_standby(&dev("sdb")), Some(false));
    assert_eq!(leitor.em_standby(&dev("sdz")), None, "disco não listado");

    estado.atualizado = agora() - 600;
    gravar(&estado, &caminho).unwrap();
    assert_eq!(
        leitor.em_standby(&dev("sda")),
        None,
        "estado de 10 min atrás"
    );

    fs::write(&caminho, "{lixo").unwrap();
    assert_eq!(leitor.em_standby(&dev("sda")), None, "corrompido");
}
