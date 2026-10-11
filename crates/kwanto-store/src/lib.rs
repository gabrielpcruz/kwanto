//! Persistência SQLite do Kwanto (WAL + pool pequeno). Toda chamada é síncrona:
//! quem chama de código assíncrono usa `spawn_blocking`.
pub mod migracoes;

use kwanto_app::portas::{
    Balde, CatalogoComponentes, ErroRepositorio, Intervalo, RepositorioConfig, RepositorioEnergia,
};
use kwanto_core::componente::{ComponenteId, DescricaoComponente, TipoComponente};
use kwanto_core::eventos::{Migracao, RegistroMinuto};
use kwanto_core::unidades::WattHora;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use std::collections::BTreeMap;
use std::path::Path;

/// Abaixo do limite de suspensão da coleta (10 s): um banco travado não pode fazer a leitura
/// ser descartada como se o PC tivesse dormido.
const ESPERA_CONEXAO: std::time::Duration = std::time::Duration::from_secs(3);

type Pool = r2d2::Pool<SqliteConnectionManager>;
type Conexao = r2d2::PooledConnection<SqliteConnectionManager>;

#[derive(Debug, Clone)]
pub struct ArmazemSqlite {
    pool: Pool,
}

fn erro(e: impl std::fmt::Display) -> ErroRepositorio {
    ErroRepositorio::Armazenamento(e.to_string())
}

impl ArmazemSqlite {
    /// Abre (criando a pasta se preciso), liga WAL e aplica as migrações do núcleo e das extensões.
    pub fn abrir(
        caminho: &Path,
        extensoes: &[(&str, &[Migracao])],
    ) -> Result<Self, ErroRepositorio> {
        if let Some(pasta) = caminho.parent() {
            std::fs::create_dir_all(pasta).map_err(erro)?;
        }
        // O modo WAL fica gravado no arquivo. Ativá-lo numa conexão única, antes do pool, evita que as
        // conexões do pool disputem a trava exclusiva dessa troca ("database is locked" na partida).
        rusqlite::Connection::open(caminho)
            .and_then(|c| c.pragma_update(None, "journal_mode", "WAL"))
            .map_err(erro)?;
        let gerente = SqliteConnectionManager::file(caminho).with_init(|c| {
            c.execute_batch("PRAGMA busy_timeout = 5000; PRAGMA synchronous = NORMAL;")
        });
        Self::com_gerente(gerente, 4, extensoes)
    }

    /// Banco em memória (uma conexão), para testes de integração.
    pub fn em_memoria() -> Result<Self, ErroRepositorio> {
        Self::com_gerente(SqliteConnectionManager::memory(), 1, &[])
    }

    fn com_gerente(
        gerente: SqliteConnectionManager,
        tamanho: u32,
        extensoes: &[(&str, &[Migracao])],
    ) -> Result<Self, ErroRepositorio> {
        let pool = r2d2::Pool::builder()
            .max_size(tamanho)
            .connection_timeout(ESPERA_CONEXAO)
            .build(gerente)
            .map_err(erro)?;
        let mut conn = pool.get().map_err(erro)?;
        migracoes::migrar(&mut conn, "kwanto", migracoes::MIGRACOES_NUCLEO).map_err(erro)?;
        migracoes::anonimizar_ids(&mut conn).map_err(erro)?;
        for (dono, m) in extensoes {
            migracoes::migrar(&mut conn, dono, m).map_err(erro)?;
        }
        Ok(Self { pool })
    }

    /// Só para testes: segura uma conexão do pool.
    #[doc(hidden)]
    pub fn conexao_para_teste(&self) -> Conexao {
        self.pool.get().unwrap_or_else(|e| panic!("pool: {e}"))
    }

    fn conn(&self) -> Result<Conexao, ErroRepositorio> {
        self.pool.get().map_err(erro)
    }
}

impl RepositorioEnergia for ArmazemSqlite {
    fn gravar_minuto(&self, r: &RegistroMinuto) -> Result<(), ErroRepositorio> {
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(erro)?;
        {
            let mut st = tx
                .prepare_cached(
                    "INSERT INTO energia VALUES (?1, ?2, ?3) ON CONFLICT(minuto, comp) DO UPDATE SET wh = wh + excluded.wh",
                )
                .map_err(erro)?;
            for (id, wh) in &r.energia {
                st.execute(params![r.minuto, id.as_str(), wh.0])
                    .map_err(erro)?;
            }
        }
        tx.execute(
            "INSERT INTO ligado VALUES (?1, ?2) ON CONFLICT(minuto) DO UPDATE SET segundos = segundos + excluded.segundos",
            params![r.minuto, r.segundos_ligado],
        )
        .map_err(erro)?;
        tx.commit().map_err(erro)
    }

    fn totais_por_componente(
        &self,
        i: Intervalo,
    ) -> Result<BTreeMap<ComponenteId, WattHora>, ErroRepositorio> {
        let conn = self.conn()?;
        let mut st = conn
            .prepare_cached("SELECT comp, SUM(wh) FROM energia WHERE minuto >= ?1 AND minuto < ?2 GROUP BY comp")
            .map_err(erro)?;
        let linhas = st
            .query_map(params![i.inicio, i.fim], |r| {
                Ok((
                    ComponenteId::new(r.get::<_, String>(0)?),
                    WattHora(r.get(1)?),
                ))
            })
            .map_err(erro)?;
        linhas.collect::<Result<_, _>>().map_err(erro)
    }

    fn segundos_ligado(&self, i: Intervalo) -> Result<f64, ErroRepositorio> {
        self.conn()?
            .query_row(
                "SELECT COALESCE(SUM(segundos), 0) FROM ligado WHERE minuto >= ?1 AND minuto < ?2",
                params![i.inicio, i.fim],
                |r| r.get(0),
            )
            .map_err(erro)
    }

    fn primeiro_minuto(&self) -> Result<Option<i64>, ErroRepositorio> {
        self.conn()?
            .query_row("SELECT MIN(minuto) FROM ligado", [], |r| r.get(0))
            .map_err(erro)
    }

    fn baldes(&self, i: Intervalo, passo: i64) -> Result<Vec<Balde>, ErroRepositorio> {
        let conn = self.conn()?;
        let mut st = conn
            .prepare_cached(
                "WITH e AS (SELECT (minuto / ?1) * ?1 AS b, SUM(wh) AS wh FROM energia
                            WHERE minuto >= ?2 AND minuto < ?3 GROUP BY b),
                      l AS (SELECT (minuto / ?1) * ?1 AS b, SUM(segundos) AS s FROM ligado
                            WHERE minuto >= ?2 AND minuto < ?3 GROUP BY b)
                 SELECT e.b, e.wh, COALESCE(l.s, 0) FROM e LEFT JOIN l ON l.b = e.b ORDER BY e.b",
            )
            .map_err(erro)?;
        let linhas = st
            .query_map(params![passo, i.inicio, i.fim], |r| {
                Ok(Balde {
                    inicio: r.get(0)?,
                    wh_dc: WattHora(r.get(1)?),
                    segundos_ligado: r.get(2)?,
                })
            })
            .map_err(erro)?;
        linhas.collect::<Result<_, _>>().map_err(erro)
    }
}

impl RepositorioConfig for ArmazemSqlite {
    fn carregar(&self) -> Result<Vec<(String, f64)>, ErroRepositorio> {
        let conn = self.conn()?;
        let mut st = conn
            .prepare_cached("SELECT chave, valor FROM config")
            .map_err(erro)?;
        let linhas = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(erro)?;
        linhas.collect::<Result<_, _>>().map_err(erro)
    }

    fn salvar(&self, pares: &[(&str, f64)]) -> Result<(), ErroRepositorio> {
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(erro)?;
        for (k, v) in pares {
            tx.execute(
                "INSERT INTO config VALUES (?1, ?2) ON CONFLICT(chave) DO UPDATE SET valor = excluded.valor",
                params![k, v],
            )
            .map_err(erro)?;
        }
        tx.commit().map_err(erro)
    }
}

impl CatalogoComponentes for ArmazemSqlite {
    fn registrar(&self, componentes: &[DescricaoComponente]) -> Result<(), ErroRepositorio> {
        let mut conn = self.conn()?;
        let tx = conn.transaction().map_err(erro)?;
        for d in componentes {
            tx.execute(
                "INSERT INTO componentes VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET rotulo = excluded.rotulo, tipo = excluded.tipo, origem = excluded.origem",
                params![d.id.as_str(), d.rotulo, d.tipo.como_texto(), d.origem],
            )
            .map_err(erro)?;
        }
        tx.commit().map_err(erro)
    }

    fn listar(&self) -> Result<Vec<DescricaoComponente>, ErroRepositorio> {
        let conn = self.conn()?;
        let mut st = conn
            .prepare_cached("SELECT id, rotulo, tipo, origem FROM componentes")
            .map_err(erro)?;
        let linhas = st
            .query_map([], |r| {
                Ok(DescricaoComponente {
                    id: ComponenteId::new(r.get::<_, String>(0)?),
                    rotulo: r.get(1)?,
                    tipo: TipoComponente::de_texto(&r.get::<_, String>(2)?),
                    origem: r.get(3)?,
                })
            })
            .map_err(erro)?;
        linhas.collect::<Result<_, _>>().map_err(erro)
    }
}
