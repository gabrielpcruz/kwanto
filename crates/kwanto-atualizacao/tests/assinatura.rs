use kwanto_atualizacao::ErroAtualizacao;
use kwanto_atualizacao::manifesto::verificar_assinatura;
use kwanto_atualizacao::versao::Versao;
use std::io::Cursor;

/// Par de chaves de teste e assinatura do conteúdo, no formato do minisign.
fn assinar(conteudo: &[u8]) -> (String, String) {
    let par = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let assinatura =
        minisign::sign(Some(&par.pk), &par.sk, Cursor::new(conteudo), None, None).unwrap();
    (
        par.pk.to_box().unwrap().into_string(),
        assinatura.into_string(),
    )
}

const MANIFESTO: &str = r#"{"versao":"0.2.0","pacotes":{"deb":{"arquivo":"kwanto_0.2.0_amd64.deb","sha256":"aa00000000000000000000000000000000000000000000000000000000000000","tamanho":10}}}"#;

#[test]
fn manifesto_com_assinatura_valida() {
    let (chave, assinatura) = assinar(MANIFESTO.as_bytes());
    let m = verificar_assinatura(MANIFESTO.as_bytes(), &assinatura, &chave).unwrap();
    assert_eq!(m.versao().unwrap(), Versao::ler("0.2.0").unwrap());
    assert_eq!(
        m.pacote_por_chave("deb").unwrap().arquivo,
        "kwanto_0.2.0_amd64.deb"
    );
}

#[test]
fn um_byte_adulterado_e_recusado() {
    let (chave, assinatura) = assinar(MANIFESTO.as_bytes());
    let adulterado = MANIFESTO.replace("0.2.0\",\"pacotes", "0.2.1\",\"pacotes");
    assert!(matches!(
        verificar_assinatura(adulterado.as_bytes(), &assinatura, &chave),
        Err(ErroAtualizacao::Assinatura)
    ));
}

#[test]
fn assinatura_de_outra_chave_e_recusada() {
    let (_, assinatura) = assinar(MANIFESTO.as_bytes());
    let (outra_chave, _) = assinar(b"qualquer");
    assert!(matches!(
        verificar_assinatura(MANIFESTO.as_bytes(), &assinatura, &outra_chave),
        Err(ErroAtualizacao::Assinatura)
    ));
}

#[test]
fn assinatura_ausente_ou_lixo_e_recusada() {
    let (chave, _) = assinar(MANIFESTO.as_bytes());
    assert!(matches!(
        verificar_assinatura(MANIFESTO.as_bytes(), "", &chave),
        Err(ErroAtualizacao::Assinatura)
    ));
    assert!(matches!(
        verificar_assinatura(MANIFESTO.as_bytes(), "lixo", &chave),
        Err(ErroAtualizacao::Assinatura)
    ));
}

#[test]
fn manifesto_assinado_mas_com_nome_perigoso_e_recusado() {
    let perigoso = MANIFESTO.replace("kwanto_0.2.0_amd64.deb", "../../etc/passwd");
    let (chave, assinatura) = assinar(perigoso.as_bytes());
    assert!(matches!(
        verificar_assinatura(perigoso.as_bytes(), &assinatura, &chave),
        Err(ErroAtualizacao::Manifesto(_))
    ));
}

#[test]
fn chave_oficial_recusa_assinatura_de_outra_chave() {
    let (_, assinatura) = assinar(MANIFESTO.as_bytes());
    assert!(
        verificar_assinatura(
            MANIFESTO.as_bytes(),
            &assinatura,
            kwanto_atualizacao::chave_oficial()
        )
        .is_err()
    );
}

#[test]
fn chave_oficial_embutida_e_uma_chave_minisign_valida() {
    assert!(minisign_verify::PublicKey::decode(kwanto_atualizacao::chave_oficial().trim()).is_ok());
}
