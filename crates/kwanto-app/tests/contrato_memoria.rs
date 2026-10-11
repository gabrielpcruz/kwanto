use kwanto_app::contrato;
use kwanto_app::memoria::RepositorioMemoria;

#[test]
fn memoria_cumpre_contrato_de_energia() {
    contrato::energia(&RepositorioMemoria::default());
}

#[test]
fn memoria_cumpre_contrato_de_config() {
    contrato::config(&RepositorioMemoria::default());
}

#[test]
fn memoria_cumpre_contrato_de_catalogo() {
    contrato::catalogo(&RepositorioMemoria::default());
}
