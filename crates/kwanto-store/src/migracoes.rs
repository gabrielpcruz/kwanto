//! Migrações numeradas por "dono" (núcleo ou extensão), aplicadas uma única vez.
use kwanto_core::eventos::Migracao;
use rusqlite::Connection;

pub const MIGRACOES_NUCLEO: &[Migracao] = &[Migracao {
    versao: 1,
    sql: "
        CREATE TABLE energia (minuto INTEGER NOT NULL, comp TEXT NOT NULL, wh REAL NOT NULL, PRIMARY KEY (minuto, comp));
        CREATE TABLE ligado (minuto INTEGER PRIMARY KEY, segundos REAL NOT NULL);
        CREATE TABLE config (chave TEXT PRIMARY KEY, valor REAL NOT NULL);
        CREATE TABLE componentes (id TEXT PRIMARY KEY, rotulo TEXT NOT NULL, tipo TEXT NOT NULL, origem TEXT NOT NULL);
    ",
}];

pub fn migrar(conn: &mut Connection, dono: &str, migracoes: &[Migracao]) -> rusqlite::Result<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS versao_esquema (dono TEXT PRIMARY KEY, versao INTEGER NOT NULL)")?;
    let atual: u32 = match conn.query_row(
        "SELECT versao FROM versao_esquema WHERE dono = ?1",
        [dono],
        |r| r.get(0),
    ) {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => 0,
        Err(e) => return Err(e),
    };
    for m in migracoes.iter().filter(|m| m.versao > atual) {
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql)?;
        tx.execute(
            "INSERT INTO versao_esquema VALUES (?1, ?2) ON CONFLICT(dono) DO UPDATE SET versao = excluded.versao",
            rusqlite::params![dono, m.versao],
        )?;
        tx.commit()?;
        tracing::info!(dono, versao = m.versao, "migração aplicada");
    }
    Ok(())
}

/// IDs gravados por versões anteriores levavam o WWID/serial do disco e o UUID da GPU. Troca por IDs
/// anônimos (`ComponenteId::anonimo`) somando a energia do mesmo minuto. Roda uma vez por banco.
pub fn anonimizar_ids(conn: &mut Connection) -> rusqlite::Result<()> {
    use kwanto_core::componente::ComponenteId;
    const DONO: &str = "kwanto-ids";
    let feita: bool = conn
        .query_row(
            "SELECT 1 FROM versao_esquema WHERE dono = ?1",
            [DONO],
            |_| Ok(true),
        )
        .or_else(|e| {
            if e == rusqlite::Error::QueryReturnedNoRows {
                Ok(false)
            } else {
                Err(e)
            }
        })?;
    if feita {
        return Ok(());
    }
    let tx = conn.transaction()?;
    let mut ids: Vec<String> = Vec::new();
    for sql in [
        "SELECT DISTINCT comp FROM energia",
        "SELECT id FROM componentes",
    ] {
        let mut st = tx.prepare(sql)?;
        ids.extend(
            st.query_map([], |r| r.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    ids.sort();
    ids.dedup();
    let mut trocados = 0;
    for antigo in ids {
        let Some((prefixo, bruto)) = ComponenteId::new(antigo.as_str())
            .bruto_identificavel()
            .map(|(p, b)| (p.to_string(), b.to_string()))
        else {
            continue;
        };
        let novo = ComponenteId::anonimo(&prefixo, &bruto);
        tx.execute(
            "INSERT INTO energia (minuto, comp, wh) SELECT minuto, ?2, wh FROM energia WHERE comp = ?1
             ON CONFLICT(minuto, comp) DO UPDATE SET wh = energia.wh + excluded.wh",
            rusqlite::params![antigo, novo.as_str()],
        )?;
        tx.execute("DELETE FROM energia WHERE comp = ?1", [&antigo])?;
        tx.execute(
            "INSERT OR IGNORE INTO componentes (id, rotulo, tipo, origem) SELECT ?2, rotulo, tipo, origem FROM componentes WHERE id = ?1",
            rusqlite::params![antigo, novo.as_str()],
        )?;
        tx.execute("DELETE FROM componentes WHERE id = ?1", [&antigo])?;
        trocados += 1;
    }
    tx.execute("INSERT INTO versao_esquema VALUES (?1, 1)", [DONO])?;
    tx.commit()?;
    if trocados > 0 {
        tracing::info!(trocados, "IDs de hardware trocados por IDs anônimos");
    }
    Ok(())
}
