//! Arredondamento na saída da API: o usuário vê 0,1 W, 0,001 kWh e R$ 0,01; a precisão fina do
//! contador de energia não sai do serviço (ela serviria de canal lateral). Zero negativo vira zero.
use serde::Serializer;

fn arred(v: f64, casas: i32) -> f64 {
    let m = 10f64.powi(casas);
    let r = (v * m).round() / m;
    if r == 0.0 { 0.0 } else { r }
}

#[allow(clippy::trivially_copy_pass_by_ref)]
pub fn uma<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(arred(*v, 1))
}

#[allow(clippy::trivially_copy_pass_by_ref)]
pub fn duas<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(arred(*v, 2))
}

#[allow(clippy::trivially_copy_pass_by_ref)]
pub fn tres<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64(arred(*v, 3))
}

#[allow(clippy::ref_option)]
pub fn opcional_duas<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(x) => s.serialize_some(&arred(*x, 2)),
        None => s.serialize_none(),
    }
}

#[allow(clippy::ref_option)]
pub fn opcional_uma<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(x) => s.serialize_some(&arred(*x, 1)),
        None => s.serialize_none(),
    }
}

#[cfg(test)]
mod testes {
    use super::arred;

    #[test]
    fn arredonda_e_tira_o_zero_negativo() {
        assert_eq!(arred(47.133_978_111_769_196, 1), 47.1);
        assert_eq!(arred(0.185_454_545, 2), 0.19);
        assert_eq!(arred(-0.000_01, 3).to_string(), "0");
        assert_eq!(arred(-0.0, 1).to_string(), "0");
    }
}
