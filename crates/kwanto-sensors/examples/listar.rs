//! Lista os componentes detectados nesta máquina: `cargo run -p kwanto-sensors --example listar`.
use kwanto_core::sensores::DetectorDeHardware;
use kwanto_sensors::detector::DetectorLinux;
use kwanto_sensors::raiz::RaizDoSistema;

fn main() {
    let hw = DetectorLinux::novo(RaizDoSistema::real()).detectar();
    println!(
        "cpu estimada: {}, hdd: {}, notebook: {}",
        hw.cpu_estimada, hw.tem_hdd, hw.notebook
    );
    for m in &hw.medidores {
        for c in m.componentes() {
            println!(
                "{:9} {:30} {:40} {}",
                c.tipo().como_texto(),
                c.rotulo,
                c.id.as_str(),
                c.origem
            );
        }
    }
}
