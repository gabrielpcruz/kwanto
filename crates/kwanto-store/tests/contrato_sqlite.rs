use kwanto_app::contrato;
use kwanto_app::portas::RepositorioConfig;
use kwanto_core::eventos::Migracao;
use kwanto_store::ArmazemSqlite;

fn novo() -> (tempfile::TempDir, ArmazemSqlite) {
    let d = tempfile::tempdir().unwrap();
    let a = ArmazemSqlite::abrir(&d.path().join("k.db"), &[]).unwrap();
    (d, a)
}

#[test]
fn sqlite_cumpre_contrato_de_energia() {
    let (_d, a) = novo();
    contrato::energia(&a);
}

#[test]
fn sqlite_cumpre_contrato_de_config() {
    let (_d, a) = novo();
    contrato::config(&a);
}

#[test]
fn sqlite_cumpre_contrato_de_catalogo() {
    let (_d, a) = novo();
    contrato::catalogo(&a);
}

#[test]
fn reabrir_preserva_dados_e_aplica_migracoes_de_extensao_uma_vez() {
    let d = tempfile::tempdir().unwrap();
    let caminho = d.path().join("k.db");
    let ext: &[Migracao] = &[Migracao {
        versao: 1,
        sql: "CREATE TABLE pro_alertas (id INTEGER PRIMARY KEY)",
    }];
    {
        let a = ArmazemSqlite::abrir(&caminho, &[("exemplo", ext)]).unwrap();
        a.salvar(&[("preco_kwh", 1.0)]).unwrap();
    }
    let a = ArmazemSqlite::abrir(&caminho, &[("exemplo", ext)]).unwrap(); // não recria a tabela
    assert_eq!(a.carregar().unwrap(), vec![("preco_kwh".to_string(), 1.0)]);
}

#[test]
fn caminho_sem_permissao_vira_erro_e_nao_panico() {
    assert!(ArmazemSqlite::abrir(std::path::Path::new("/proc/kwanto/k.db"), &[]).is_err());
}

#[test]
fn banco_novo_fica_em_modo_wal_gravado_no_arquivo() {
    let d = tempfile::tempdir().unwrap();
    let caminho = d.path().join("k.db");
    drop(ArmazemSqlite::abrir(&caminho, &[]).unwrap());
    let modo: String = rusqlite::Connection::open(&caminho)
        .unwrap()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(modo, "wal");
}

#[test]
fn banco_ocupado_desiste_rapido_em_vez_de_travar_a_coleta() {
    use kwanto_app::portas::{Intervalo, RepositorioEnergia};
    let (_d, a) = novo();
    // segura todas as conexões do pool, como um banco travado faria
    let presas: Vec<_> = (0..4)
        .map(|_| a.clone())
        .map(|c| c.conexao_para_teste())
        .collect();
    let inicio = std::time::Instant::now();
    assert!(
        a.totais_por_componente(Intervalo { inicio: 0, fim: 1 })
            .is_err()
    );
    assert!(
        inicio.elapsed() < std::time::Duration::from_secs(5),
        "esperou {:?}",
        inicio.elapsed()
    );
    drop(presas);
}

/// Banco de uma versão anterior, com o WWID do disco e o UUID da GPU nos IDs: ao abrir, os IDs
/// viram anônimos e o histórico continua somando o mesmo (inclusive se o ID novo já tinha dados).
#[test]
fn ids_antigos_com_identificador_do_hardware_viram_anonimos() {
    use kwanto_app::portas::{Intervalo, RepositorioEnergia};
    use kwanto_core::componente::ComponenteId;
    let d = tempfile::tempdir().unwrap();
    let caminho = d.path().join("k.db");
    drop(ArmazemSqlite::abrir(&caminho, &[]).unwrap());
    let disco = ComponenteId::anonimo("disco", "naa.0000000000000001");
    {
        let c = rusqlite::Connection::open(&caminho).unwrap();
        c.execute_batch(&format!(
            "INSERT INTO energia VALUES (60, 'disco:naa.0000000000000001', 1.0), (120, 'disco:naa.0000000000000001', 2.0),
                                        (120, '{disco}', 0.5), (60, 'gpu:GPU-abc', 4.0), (60, 'gpu:0000:01:00.0', 8.0), (60, 'ram', 1.0);
             INSERT INTO componentes VALUES ('disco:naa.0000000000000001', 'HDD X', 'estimado', 'atividade'),
                                            ('gpu:GPU-abc', 'GPU', 'medido', 'NVML');
             DELETE FROM versao_esquema WHERE dono = 'kwanto-ids';",
            disco = disco.as_str()
        ))
        .unwrap();
    }
    let a = ArmazemSqlite::abrir(&caminho, &[]).unwrap();
    let total = a
        .totais_por_componente(Intervalo {
            inicio: 0,
            fim: 1_000,
        })
        .unwrap();
    let ids: Vec<&str> = total.keys().map(ComponenteId::as_str).collect();
    for id in &ids {
        assert!(
            !id.contains("naa.") && !id.contains("GPU-abc"),
            "{id} ainda tem o identificador"
        );
    }
    assert_eq!(total[&disco].0, 3.5, "1 + 2 + 0,5");
    assert_eq!(total[&ComponenteId::anonimo("gpu", "GPU-abc")].0, 4.0);
    assert_eq!(
        total[&ComponenteId::new("gpu:0000:01:00.0")].0,
        8.0,
        "endereço PCI não muda"
    );
    assert_eq!(total[&ComponenteId::new("ram")].0, 1.0);
    let c = rusqlite::Connection::open(&caminho).unwrap();
    let antigos: i64 = c
        .query_row(
            "SELECT COUNT(*) FROM componentes WHERE id LIKE 'disco:naa.%' OR id LIKE 'gpu:GPU-%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(antigos, 0);
}
