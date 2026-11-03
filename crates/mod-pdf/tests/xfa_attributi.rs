//! Revisione del 31/10, punto 4 (CVE-2026-92987): roxmltree, che PDFCraft usa per i moduli XFA dei
//! PDF, confrontava a due a due gli attributi di un elemento, e centomila attributi tenevano la
//! CPU al 100% per minuti. La copia in vendor/roxmltree si ferma a 1024 attributi per elemento.

#[test]
fn un_elemento_con_migliaia_di_attributi_si_ferma() {
    let attributi: String = (0..5_000).map(|i| format!(" a{i}=\"x\"")).collect();
    let xml = format!("<campo{attributi}/>");
    let inizio = std::time::Instant::now();
    let letto = roxmltree::Document::parse(&xml);
    assert!(matches!(letto, Err(roxmltree::Error::AttributesLimitReached)), "{letto:?}");
    assert!(inizio.elapsed() < std::time::Duration::from_secs(1));
    let pochi: String = (0..100).map(|i| format!(" a{i}=\"x\"")).collect();
    let xml = format!("<campo{pochi}/>");
    assert!(roxmltree::Document::parse(&xml).is_ok());
}
