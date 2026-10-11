//! Dono (UID) de uma conexão TCP local, pela tabela de sockets do kernel (/proc/net/tcp e tcp6).
//! O serviço usa para saber quem o chama; a bandeja, para saber se quem responde é mesmo o serviço.
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

/// Endereço como o kernel escreve: IPv4 em 8 dígitos hexadecimais e IPv6 em 32, cada palavra de
/// 32 bits na ordem de bytes da máquina; porta em hexadecimal.
fn ler_endereco(texto: &str) -> Option<SocketAddr> {
    let (ip, porta) = texto.split_once(':')?;
    let porta = u16::from_str_radix(porta, 16).ok()?;
    let palavra = |i: usize| {
        u32::from_str_radix(ip.get(i * 8..i * 8 + 8)?, 16)
            .ok()
            .map(u32::to_ne_bytes)
    };
    let ip = match ip.len() {
        8 => IpAddr::V4(Ipv4Addr::from(palavra(0)?)),
        32 => {
            let mut b = [0u8; 16];
            for i in 0..4 {
                b[i * 4..i * 4 + 4].copy_from_slice(&palavra(i)?);
            }
            IpAddr::V6(Ipv6Addr::from(b))
        }
        _ => return None,
    };
    Some(SocketAddr::new(ip, porta))
}

/// Estado ESTABLISHED na tabela do kernel.
const ESTABELECIDA: &str = "01";

/// UID do socket com este endereço local e este endereço remoto, numa tabela no formato do kernel.
/// Só conexões vivas (ESTABLISHED): um socket fechado pelo dono fica órfão e o kernel o atribui ao
/// UID 0, o que faria qualquer processo parecer root.
#[must_use]
pub fn dono_na_tabela(tabela: &str, local: SocketAddr, remoto: SocketAddr) -> Option<u32> {
    tabela.lines().skip(1).find_map(|linha| {
        let campos: Vec<&str> = linha.split_whitespace().collect();
        let (l, r, estado, uid) = (
            campos.get(1)?,
            campos.get(2)?,
            campos.get(3)?,
            campos.get(7)?,
        );
        (*estado == ESTABELECIDA && ler_endereco(l)? == local && ler_endereco(r)? == remoto)
            .then(|| uid.parse().ok())?
    })
}

/// UID dono do socket `local` conectado a `remoto`, nesta máquina.
#[must_use]
pub fn dono(local: SocketAddr, remoto: SocketAddr) -> Option<u32> {
    ["/proc/net/tcp", "/proc/net/tcp6"]
        .iter()
        .filter_map(|caminho| std::fs::read_to_string(caminho).ok())
        .find_map(|tabela| dono_na_tabela(&tabela, local, remoto))
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::os::unix::fs::MetadataExt as _;

    const TCP: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:2253 00000000:0000 0A 00000000:00000000 00:00000000 00000000   998        0 1 1 0 100 0 0 10 0
   1: 0100007F:2253 0100007F:C350 01 00000000:00000000 00:00000000 00000000   998        0 2 1 0 20 4 30 10 -1
   2: 0100007F:C350 0100007F:2253 01 00000000:00000000 00:00000000 00000000  1000        0 3 1 0 20 4 30 10 -1
   3: 0100007F:C351 0100007F:2253 01 00000000:00000000 00:00000000 00000000  1001        0 4 1 0 20 4 30 10 -1
";

    fn end(porta: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], porta))
    }

    #[test]
    fn os_dois_lados_de_uma_conexao() {
        assert_eq!(
            dono_na_tabela(TCP, end(50_000), end(8787)),
            Some(1000),
            "cliente"
        );
        assert_eq!(
            dono_na_tabela(TCP, end(8787), end(50_000)),
            Some(998),
            "servidor"
        );
        assert_eq!(dono_na_tabela(TCP, end(50_001), end(8787)), Some(1001));
        assert_eq!(
            dono_na_tabela(TCP, end(50_002), end(8787)),
            None,
            "inexistente"
        );
        assert_eq!(
            dono_na_tabela("lixo\n1: x y\n", end(50_000), end(8787)),
            None
        );
    }

    #[test]
    fn enderecos_ipv6_no_formato_do_kernel() {
        let tcp6 = "  sl  local_address rem_address st tx rx tr retr uid
   0: 00000000000000000000000001000000:C350 00000000000000000000000001000000:2253 01 0:0 0:0 0 1000 0 1
   1: 0000000000000000FFFF00000100007F:C351 0000000000000000FFFF00000100007F:2253 01 0:0 0:0 0 1002 0 2
";
        let v6 = |p| SocketAddr::from((Ipv6Addr::LOCALHOST, p));
        assert_eq!(dono_na_tabela(tcp6, v6(50_000), v6(8787)), Some(1000));
        let mapeado = |p| SocketAddr::from((Ipv4Addr::LOCALHOST.to_ipv6_mapped(), p));
        assert_eq!(
            dono_na_tabela(tcp6, mapeado(50_001), mapeado(8787)),
            Some(1002)
        );
    }

    /// Socket fechado pelo dono fica órfão (FIN_WAIT/TIME_WAIT) e o kernel o atribui ao UID 0.
    /// Só conexões vivas (ESTABLISHED) identificam alguém; o resto é "dono desconhecido".
    #[test]
    fn so_conexao_estabelecida_tem_dono() {
        let orfao = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:C350 0100007F:2253 06 00000000:00000000 03:00000F9F 00000000     0        0 0 3 0
   1: 0100007F:C351 0100007F:2253 05 00000000:00000000 00:00000000 00000000     0        0 0 3 0
";
        assert_eq!(
            dono_na_tabela(orfao, end(50_000), end(8787)),
            None,
            "TIME_WAIT"
        );
        assert_eq!(
            dono_na_tabela(orfao, end(50_001), end(8787)),
            None,
            "FIN_WAIT2"
        );
    }

    #[test]
    fn conexao_fechada_pelo_cliente_nao_vira_root() {
        let ouvinte = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let servidor = ouvinte.local_addr().unwrap();
        let cliente = std::net::TcpStream::connect(servidor).unwrap();
        let local = cliente.local_addr().unwrap();
        let (_aceita, _) = ouvinte.accept().unwrap();
        drop(cliente);
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert_eq!(
            dono(local, servidor),
            None,
            "socket órfão não pode ser atribuído a ninguém"
        );
    }

    #[test]
    fn conexao_real_os_dois_lados_sao_deste_usuario() {
        let ouvinte = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let servidor = ouvinte.local_addr().unwrap();
        let cliente = std::net::TcpStream::connect(servidor).unwrap();
        let local = cliente.local_addr().unwrap();
        // Depois do accept: antes dele, kernels mais antigos mostram o lado do servidor como do root.
        let (_aceita, _) = ouvinte.accept().unwrap();
        let meu = std::fs::metadata("/proc/self").unwrap().uid();
        assert_eq!(dono(local, servidor), Some(meu), "lado do cliente");
        assert_eq!(dono(servidor, local), Some(meu), "lado do servidor");
    }
}
