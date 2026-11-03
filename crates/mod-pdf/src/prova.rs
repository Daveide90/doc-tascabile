//! PDF sintetici per le prove, scritti qui oggetto per oggetto: mai documenti veri.

/// Una pagina di prova: misura della MediaBox in punti, `/Rotate` e flusso di contenuto (gli
/// operatori del PDF, per esempio `1 0 0 rg 0 0 50 50 re f` per un quadrato rosso in basso a
/// sinistra).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pagina<'a> {
    pub larghezza: f32,
    pub altezza: f32,
    pub rotazione: u16,
    pub contenuto: &'a str,
}

impl Pagina<'static> {
    /// Una pagina vuota, diritta.
    pub fn bianca(larghezza: f32, altezza: f32) -> Self {
        Self { larghezza, altezza, rotazione: 0, contenuto: "" }
    }
}

/// Le risorse di ogni pagina: Helvetica (F1) e Helvetica grassetto (F2), due dei 14 caratteri
/// standard, che il motore disegna coi suoi sostituti senza averli nel file.
const RISORSE: &str = "<< /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >> \
                       /F2 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >> >> >>";

/// Un segnalibro del PDF di prova: titolo, pagina (da 0), punto in pt dal fondo della pagina
/// (`/XYZ`, o la cima senza) e figli.
#[derive(Debug, Clone, PartialEq)]
pub struct Segnalibro<'a> {
    pub titolo: &'a str,
    pub pagina: usize,
    pub y: Option<f32>,
    pub figli: Vec<Segnalibro<'a>>,
}

/// Dove porta un link del PDF di prova.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bersaglio<'a> {
    /// Una pagina (da 0) e il punto in pt dal fondo, se c'è.
    Pagina(usize, Option<f32>),
    /// Un indirizzo (`/URI`).
    Indirizzo(&'a str),
}

/// Un link del PDF di prova: pagina (da 0), rettangolo in punti del PDF (dal basso a sinistra) e
/// dove porta.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Collegamento<'a> {
    pub pagina: usize,
    pub rettangolo: [f32; 4],
    pub bersaglio: Bersaglio<'a>,
}

/// Un PDF 1.7 con queste pagine: catalogo, albero delle pagine, una pagina e un flusso per
/// ognuna, tabella xref con le posizioni vere (il motore non deve ripararlo).
pub fn pdf(pagine: &[Pagina<'_>]) -> Vec<u8> {
    pdf_completo(pagine, &[], &[])
}

/// Come [`pdf`], coi segnalibri (`/Outlines`) e i link nelle pagine (`/Annots` di tipo `/Link`).
pub fn pdf_completo(pagine: &[Pagina<'_>], segnalibri: &[Segnalibro<'_>], collegamenti: &[Collegamento<'_>]) -> Vec<u8> {
    scrivi(pagine, segnalibri, collegamenti, None)
}

/// Come [`pdf`], col titolo e l'autore nel dizionario `/Info`.
pub fn pdf_con_proprieta(pagine: &[Pagina<'_>], titolo: &str, autore: &str) -> Vec<u8> {
    scrivi(pagine, &[], &[], Some((titolo, autore)))
}

/// Un PDF di una pagina A4 con «Pagina protetta» scritto, cifrato in AES-256 dal motore con
/// `password` per aprirlo.
pub fn cifrato(password: &str) -> Vec<u8> {
    use std::sync::Arc;
    let pagina = Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: "BT /F1 24 Tf 60 700 Td (Pagina protetta) Tj ET" };
    let mut s = pdfcraft_engine::Session::new();
    let id = s.open("prova.pdf", None, Arc::new(pdf(&[pagina])), None).expect("il PDF di prova si apre");
    let protezione = pdfcraft_engine::Protection { open_password: Some(password.into()), ..Default::default() };
    s.apply(id, pdfcraft_engine::Edit::Protect(protezione)).expect("il motore lo cifra");
    s.save_bytes(id).expect("il motore lo salva").to_vec()
}

/// `byte` firmati dal motore con un'identità fatta qui (chiave P-256, certificato firmato da sé,
/// «Firmatario di prova»): una firma digitale invisibile, integra ma di qualcuno che il telefono
/// non conosce. Con `certificazione` è una certificazione col permesso DocMDP dato (1 nessuna
/// modifica, 2 moduli, 3 moduli e commenti), senza è una firma di approvazione.
pub fn firmato(byte: Vec<u8>, certificazione: Option<u8>) -> Vec<u8> {
    use pdfcraft_engine::sign::{Certificate, DigitalId, Name, PrivateKey, Time};
    use std::sync::Arc;
    let mut s = pdfcraft_engine::Session::new();
    let id = s.open("prova.pdf", None, Arc::new(byte), None).expect("il PDF di prova si apre");
    let chiave = PrivateKey::generate_p256().expect("chiave di prova");
    let nome = Name::build("Firmatario di prova", "", "", "", "IT");
    let certificato = Certificate::self_signed(&nome, &chiave, Time::from_unix(1_792_738_800), 5, &[1, 2, 3]).expect("certificato di prova");
    let identita = DigitalId { key: chiave, certificate: certificato, chain: Vec::new(), friendly_name: None };
    let opzioni = pdfcraft_engine::SignOptions { certify: certificazione, ..Default::default() };
    s.sign(id, &identita, opzioni).expect("il motore firma").to_vec()
}

/// Una nota (l'icona del commento, senza testo né autore) sulla pagina `pagina` (da 0), col suo
/// angolo in `at` in punti del PDF dal basso a sinistra: la modifica di prova del lavoro sul PDF,
/// finché i commenti dell'utente non arrivano (piano «PDF completo», P5).
pub fn nota(pagina: usize, at: [f64; 2]) -> crate::Modifica {
    use pdfcraft_engine::{Edit, NewAnnotation, NoteIcon, Shape, Style};
    let shape = Shape::Note { at, icon: NoteIcon::Comment };
    let style = Style::default_for(&shape);
    crate::Modifica::del_motore(Edit::AddAnnotation(NewAnnotation { page: pagina, shape, style, contents: String::new(), author: String::new() }))
}

fn scrivi(pagine: &[Pagina<'_>], segnalibri: &[Segnalibro<'_>], collegamenti: &[Collegamento<'_>], proprieta: Option<(&str, &str)>) -> Vec<u8> {
    let n = pagine.len();
    let pagina_obj = |i: usize| 3 + 2 * i;
    // Dopo pagine e flussi: la radice dei segnalibri e i segnalibri, poi i link.
    let radice = 3 + 2 * n;
    let mut voci: Vec<(usize, String)> = Vec::new();
    let mut prossimo = radice + 1;
    let (primo, ultimo, quanti) = scrivi_segnalibri(segnalibri, radice, &mut prossimo, &mut voci, &pagina_obj);
    let primo_link = if segnalibri.is_empty() { radice } else { prossimo };
    let figlie: Vec<String> = (0..n).map(|i| format!("{} 0 R", pagina_obj(i))).collect();
    let segnalibri_nel_catalogo = if segnalibri.is_empty() { String::new() } else { format!(" /Outlines {radice} 0 R /PageMode /UseOutlines") };
    let mut oggetti = vec![
        format!("<< /Type /Catalog /Pages 2 0 R{segnalibri_nel_catalogo} >>"),
        format!("<< /Type /Pages /Kids [{}] /Count {} >>", figlie.join(" "), n),
    ];
    for (i, p) in pagine.iter().enumerate() {
        let link: Vec<String> =
            collegamenti.iter().enumerate().filter(|(_, l)| l.pagina == i).map(|(k, _)| format!("{} 0 R", primo_link + k)).collect();
        let annots = if link.is_empty() { String::new() } else { format!(" /Annots [{}]", link.join(" ")) };
        oggetti.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Rotate {} /Resources {RISORSE} /Contents {} 0 R{annots} >>",
            p.larghezza,
            p.altezza,
            p.rotazione,
            4 + 2 * i
        ));
        oggetti.push(format!("<< /Length {} >>\nstream\n{}\nendstream", p.contenuto.len(), p.contenuto));
    }
    if !segnalibri.is_empty() {
        oggetti.push(format!("<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {quanti} >>", primo.unwrap_or(0), ultimo.unwrap_or(0)));
        voci.sort_by_key(|(k, _)| *k);
        oggetti.extend(voci.into_iter().map(|(_, v)| v));
    }
    for l in collegamenti {
        let [x0, y0, x1, y1] = l.rettangolo;
        let azione = match l.bersaglio {
            Bersaglio::Pagina(p, y) => format!("/Dest [{} 0 R /XYZ null {} null]", pagina_obj(p), y.map_or("null".to_string(), |y| y.to_string())),
            Bersaglio::Indirizzo(u) => format!("/A << /S /URI /URI {} >>", stringa(u)),
        };
        oggetti.push(format!("<< /Type /Annot /Subtype /Link /Rect [{x0} {y0} {x1} {y1}] /Border [0 0 0] {azione} >>"));
    }
    let info = proprieta.map(|(titolo, autore)| {
        oggetti.push(format!("<< /Title {} /Author {} >>", stringa(titolo), stringa(autore)));
        format!(" /Info {} 0 R", oggetti.len())
    });
    let mut uscita = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut posti = Vec::with_capacity(oggetti.len());
    for (i, o) in oggetti.iter().enumerate() {
        posti.push(uscita.len());
        uscita.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = uscita.len();
    uscita.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", oggetti.len() + 1).as_bytes());
    for p in posti {
        uscita.extend_from_slice(format!("{p:010} 00000 n \n").as_bytes());
    }
    let info = info.unwrap_or_default();
    uscita.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R{info} >>\nstartxref\n{xref}\n%%EOF\n", oggetti.len() + 1).as_bytes());
    uscita
}

/// Scrive i segnalibri di un livello sotto `genitore`, coi loro figli, numerandoli da `prossimo`:
/// dà il primo, l'ultimo e quanti sono con tutti i discendenti (tutti aperti).
fn scrivi_segnalibri(
    lista: &[Segnalibro<'_>],
    genitore: usize,
    prossimo: &mut usize,
    fuori: &mut Vec<(usize, String)>,
    pagina_obj: &dyn Fn(usize) -> usize,
) -> (Option<usize>, Option<usize>, usize) {
    let numeri: Vec<usize> = lista
        .iter()
        .map(|_| {
            *prossimo += 1;
            *prossimo - 1
        })
        .collect();
    let mut totale = 0;
    for (j, s) in lista.iter().enumerate() {
        let (primo, ultimo, figli) = scrivi_segnalibri(&s.figli, numeri[j], prossimo, fuori, pagina_obj);
        let y = s.y.map_or("null".to_string(), |y| y.to_string());
        let mut d = format!("<< /Title {} /Parent {genitore} 0 R /Dest [{} 0 R /XYZ null {y} null]", stringa(s.titolo), pagina_obj(s.pagina));
        if j > 0 {
            d.push_str(&format!(" /Prev {} 0 R", numeri[j - 1]));
        }
        if j + 1 < lista.len() {
            d.push_str(&format!(" /Next {} 0 R", numeri[j + 1]));
        }
        if let (Some(p), Some(u)) = (primo, ultimo) {
            d.push_str(&format!(" /First {p} 0 R /Last {u} 0 R /Count {figli}"));
        }
        d.push_str(" >>");
        fuori.push((numeri[j], d));
        totale += 1 + figli;
    }
    (numeri.first().copied(), numeri.last().copied(), totale)
}

/// Le larghezze di Helvetica e Helvetica grassetto in millesimi di em (dalle metriche standard),
/// per centrare i titoli del modulo; un carattere che manca vale 556.
fn larghezza(testo: &str, grassetto: bool, misura: f32) -> f32 {
    const NORMALE: &[(char, u16)] = &[
        (' ', 278),
        ('/', 278),
        ('.', 278),
        (',', 278),
        ('A', 667),
        ('C', 722),
        ('D', 722),
        ('E', 667),
        ('F', 611),
        ('I', 278),
        ('L', 556),
        ('M', 833),
        ('N', 722),
        ('O', 778),
        ('P', 667),
        ('R', 722),
        ('S', 667),
        ('T', 611),
        ('a', 556),
        ('b', 556),
        ('c', 500),
        ('d', 556),
        ('e', 556),
        ('f', 278),
        ('g', 556),
        ('h', 556),
        ('i', 222),
        ('l', 222),
        ('m', 833),
        ('n', 556),
        ('o', 556),
        ('p', 556),
        ('r', 333),
        ('s', 500),
        ('t', 278),
        ('u', 556),
        ('v', 500),
        ('z', 500),
        ('\u{b7}', 278),
    ];
    const GRASSETTO: &[(char, u16)] = &[
        (' ', 278),
        ('A', 722),
        ('C', 722),
        ('D', 722),
        ('E', 667),
        ('F', 611),
        ('I', 278),
        ('L', 611),
        ('M', 833),
        ('N', 722),
        ('O', 778),
        ('P', 667),
        ('R', 722),
        ('S', 667),
        ('T', 611),
        ('a', 556),
        ('d', 611),
        ('e', 556),
        ('i', 278),
        ('l', 278),
        ('m', 889),
        ('n', 611),
        ('o', 611),
        ('p', 611),
        ('r', 389),
        ('s', 556),
        ('t', 333),
        ('u', 611),
        ('z', 500),
    ];
    let tabella = if grassetto { GRASSETTO } else { NORMALE };
    testo.chars().map(|c| tabella.iter().find(|(k, _)| *k == c).map_or(556, |(_, w)| *w) as f32).sum::<f32>() * misura / 1000.0
}

/// Una stringa del PDF in WinAnsi: le lettere accentate e il punto in mezzo in ottale.
fn stringa(testo: &str) -> String {
    let mut fuori = String::from("(");
    for c in testo.chars() {
        match c {
            '(' | ')' | '\\' => {
                fuori.push('\\');
                fuori.push(c);
            }
            c if c.is_ascii() => fuori.push(c),
            c if (c as u32) < 256 => fuori.push_str(&format!("\\{:03o}", c as u32)),
            _ => fuori.push('?'),
        }
    }
    fuori.push(')');
    fuori
}

/// Il contenuto di una pagina scritto in «px» del bozzetto 7 (la pagina A4 è larga 380), dall'alto
/// a sinistra: lo si porta in punti del PDF con una matrice sola.
struct Foglio {
    flusso: String,
}

/// Punti del PDF per px del bozzetto: 595 / 380.
const PX: f32 = 595.0 / 380.0;
/// Il colore del testo (`.pagina { color: #222 }`) e quello del sottotitolo (#555).
const TESTO: [f32; 3] = [0.133, 0.133, 0.133];
const GRIGIO: [f32; 3] = [0.333, 0.333, 0.333];
const BLU: [f32; 3] = [0.137, 0.275, 0.627];
const ROSSO: [f32; 3] = [0.761, 0.255, 0.169];

impl Foglio {
    fn new() -> Self {
        Self { flusso: format!("{PX:.6} 0 0 {:.6} 0 842 cm\n", -PX) }
    }

    fn testo(&mut self, x: f32, base: f32, misura: f32, grassetto: bool, colore: [f32; 3], testo: &str) {
        let [r, g, b] = colore;
        let font = if grassetto { "F2" } else { "F1" };
        self.flusso.push_str(&format!("BT {r} {g} {b} rg /{font} {misura} Tf 1 0 0 -1 {x} {base} Tm {} Tj ET\n", stringa(testo)));
    }

    fn centrato(&mut self, base: f32, misura: f32, grassetto: bool, colore: [f32; 3], testo: &str) {
        let x = (380.0 - larghezza(testo, grassetto, misura)) / 2.0;
        self.testo(x, base, misura, grassetto, colore, testo);
    }

    /// Un rettangolo con gli angoli tondi: pieno di `fondo`, col bordo `bordo` (spessore e colore),
    /// tratteggiato se `tratto`.
    fn riquadro(&mut self, posto: egui::Rect, raggio: f32, fondo: [f32; 3], bordo: (f32, [f32; 3]), tratto: bool) {
        let k = raggio * 0.5523;
        let (x, y, x1, y1) = (posto.left(), posto.top(), posto.right(), posto.bottom());
        let percorso = format!(
            "{} {y} m {} {y} l {} {y} {x1} {} {x1} {} c {x1} {} l {x1} {} {} {y1} {} {y1} c {} {y1} l {} {y1} {x} {} {x} {} c {x} {} l {x} {} {} {y} {} {y} c h",
            x + raggio,
            x1 - raggio,
            x1 - raggio + k,
            y + raggio - k,
            y + raggio,
            y1 - raggio,
            y1 - raggio + k,
            x1 - raggio + k,
            x1 - raggio,
            x + raggio,
            x + raggio - k,
            y1 - raggio + k,
            y1 - raggio,
            y + raggio,
            y + raggio - k,
            x + raggio - k,
            x + raggio
        );
        let [r, g, b] = fondo;
        let (spessore, [br, bg, bb]) = bordo;
        let trattino = if tratto { "[3 2] 0 d" } else { "[] 0 d" };
        self.flusso.push_str(&format!("{r} {g} {b} rg {br} {bg} {bb} RG {spessore} w {trattino} {percorso} B [] 0 d\n"));
    }

    /// Un campo del modulo: pieno (bordo blu, col valore) o da riempire (tratteggiato).
    fn campo(&mut self, x: f32, cima: f32, w: f32, valore: Option<&str>) {
        match valore {
            Some(v) => {
                self.riquadro(egui::Rect::from_min_size(egui::pos2(x, cima), egui::vec2(w, 20.0)), 4.0, [0.941, 0.949, 0.973], (1.5, BLU), false);
                self.testo(x + 7.0, cima + 14.0, 11.0, false, [0.118, 0.106, 0.094], v);
            }
            None => self.riquadro(
                egui::Rect::from_min_size(egui::pos2(x, cima), egui::vec2(w, 20.0)),
                4.0,
                [0.957, 0.965, 0.980],
                (1.0, [0.576, 0.651, 0.839]),
                true,
            ),
        }
    }

    /// Una riga del modulo, alta 20 px, fra 28 e 352 px: etichette e campi (con la parte dello
    /// spazio che prendono), 6 px fra un pezzo e l'altro, come `.riga-c`.
    fn riga(&mut self, cima: f32, pezzi: &[(&str, f32, Option<&str>)]) {
        self.riga_con_posti(cima, pezzi, true);
    }

    /// Come [`riga`](Self::riga); con `campi` falso disegna solo le etichette. Restituisce i posti
    /// dei campi, in px del bozzetto.
    fn riga_con_posti(&mut self, cima: f32, pezzi: &[(&str, f32, Option<&str>)], campi: bool) -> Vec<egui::Rect> {
        let etichette: f32 = pezzi.iter().filter(|p| p.1 == 0.0).map(|p| larghezza(p.0, false, 9.5)).sum();
        let parti: f32 = pezzi.iter().map(|p| p.1).sum();
        let resto = 324.0 - etichette - 6.0 * (pezzi.len() - 1) as f32;
        let mut x = 28.0;
        let mut posti = Vec::new();
        for (testo, parte, valore) in pezzi {
            if *parte == 0.0 {
                self.testo(x, cima + 17.0, 9.5, false, TESTO, testo);
                x += larghezza(testo, false, 9.5) + 6.0;
            } else {
                let w = resto * parte / parti;
                if campi {
                    self.campo(x, cima, w, *valore);
                }
                posti.push(egui::Rect::from_min_size(egui::pos2(x, cima), egui::vec2(w, 20.0)));
                x += w + 6.0;
            }
        }
        posti
    }

    fn casella(&mut self, cima: f32, segnata: bool, testo: &str) {
        if segnata {
            self.riquadro(egui::Rect::from_min_size(egui::pos2(28.0, cima), egui::vec2(13.0, 13.0)), 0.0, BLU, (1.0, BLU), false);
            self.flusso.push_str(&format!("1 1 1 RG 1.6 w 31 {} m 34 {} l 38.5 {} l S\n", cima + 6.8, cima + 9.8, cima + 3.4));
        } else {
            self.riquadro(
                egui::Rect::from_min_size(egui::pos2(28.0, cima), egui::vec2(13.0, 13.0)),
                0.0,
                [1.0, 1.0, 1.0],
                (1.0, [0.467, 0.467, 0.467]),
                false,
            );
        }
        self.testo(45.0, cima + 10.5, 9.5, false, TESTO, testo);
    }
}

/// Un modulo di tre pagine A4 come quello del bozzetto 7, per le prove a schermo: il consenso
/// informato coi campi (riempiti e da riempire), le caselle, il riquadro della firma; poi due
/// pagine di note. Testo inventato.
pub fn modulo() -> Vec<u8> {
    let mut f = Foglio::new();
    f.centrato(44.0, 13.0, true, TESTO, "CONSENSO INFORMATO AL TRATTAMENTO");
    f.centrato(61.5, 8.5, false, GRIGIO, "Modulo 2 \u{b7} da compilare a cura del paziente o del tutore");
    f.riga(88.2, &[("Il/La sottoscritto/a", 0.0, None), ("", 1.0, Some("Paziente A"))]);
    f.riga(117.2, &[("nato/a a", 0.0, None), ("", 1.0, Some("Roma")), ("il", 0.0, None), ("", 0.6, Some("12/03/1958"))]);
    f.riga(146.2, &[("residente in", 0.0, None), ("", 1.0, None)]);
    for (i, riga) in [
        "dichiara di essere stato/a informato/a in modo chiaro e comprensibile sulla",
        "natura del trattamento, sui benefici attesi, sui rischi e sulle alternative",
        "possibili, e di aver potuto porre domande ricevendo risposte esaurienti.",
    ]
    .into_iter()
    .enumerate()
    {
        f.testo(28.0, 188.0 + 13.775 * i as f32, 9.5, false, TESTO, riga);
    }
    f.casella(228.5, true, "acconsente al trattamento proposto");
    f.casella(248.3, false, "non acconsente al trattamento proposto");
    f.riga(273.1, &[("Data", 0.0, None), ("", 0.5, Some("24/10/2026")), ("Luogo", 0.0, None), ("", 1.0, None)]);
    f.riquadro(egui::Rect::from_min_size(egui::pos2(28.0, 309.1), egui::vec2(324.0, 62.0)), 8.0, [0.988, 0.953, 0.945], (1.5, ROSSO), true);
    f.centrato(344.5, 13.0, true, ROSSO, "Firma del paziente o del tutore");
    let prima = f.flusso;
    let note = |titolo: &str| {
        let mut f = Foglio::new();
        f.centrato(44.0, 13.0, true, TESTO, titolo);
        for i in 0..18 {
            let riga = if i % 3 == 2 {
                "dei rischi e delle alternative, con le risposte alle domande."
            } else {
                "Il medico illustra la natura del trattamento, i benefici attesi e la durata,"
            };
            f.testo(28.0, 76.0 + 15.0 * i as f32, 9.5, false, TESTO, riga);
        }
        f.flusso
    };
    let (seconda, terza) = (note("NOTE INFORMATIVE"), note("ALLEGATO"));
    let pagina = |contenuto| Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto };
    // L'indice: il consenso con le sue parti, poi le note e l'allegato.
    let voce = |titolo, pagina, y: Option<f32>, figli| Segnalibro { titolo, pagina, y, figli };
    let segnalibri = [
        voce(
            "Consenso informato al trattamento",
            0,
            None,
            vec![
                voce("Dati del paziente", 0, Some(700.0), vec![]),
                voce("Dichiarazione", 0, Some(560.0), vec![]),
                voce("Firma", 0, Some(360.0), vec![]),
            ],
        ),
        voce("Note informative", 1, None, vec![]),
        voce("Allegato", 2, None, vec![]),
    ];
    scrivi(&[pagina(&prima), pagina(&seconda), pagina(&terza)], &segnalibri, &[], Some(("Consenso informato al trattamento", "Ambulatorio di prova")))
}

/// I campi del modulo da compilare ([`modulo_da_compilare`]), col suggerimento (`/TU`) che li
/// distingue nelle prove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CampoDiProva {
    Sottoscritto,
    LuogoDiNascita,
    DataDiNascita,
    Residente,
    Acconsente,
    NonAcconsente,
    Data,
    Luogo,
    Reparto,
    Ricovero,
    Allergie,
    Note,
}

impl CampoDiProva {
    pub const TUTTI: [CampoDiProva; 12] = [
        CampoDiProva::Sottoscritto,
        CampoDiProva::LuogoDiNascita,
        CampoDiProva::DataDiNascita,
        CampoDiProva::Residente,
        CampoDiProva::Acconsente,
        CampoDiProva::NonAcconsente,
        CampoDiProva::Data,
        CampoDiProva::Luogo,
        CampoDiProva::Reparto,
        CampoDiProva::Ricovero,
        CampoDiProva::Allergie,
        CampoDiProva::Note,
    ];

    /// Il suggerimento del campo, che l'app mostra all'accessibilità.
    pub fn suggerimento(self) -> &'static str {
        match self {
            CampoDiProva::Sottoscritto => "Il/La sottoscritto/a",
            CampoDiProva::LuogoDiNascita => "Nato/a a",
            CampoDiProva::DataDiNascita => "Data di nascita",
            CampoDiProva::Residente => "Residente in",
            CampoDiProva::Acconsente => "Acconsente",
            CampoDiProva::NonAcconsente => "Non acconsente",
            CampoDiProva::Data => "Data",
            CampoDiProva::Luogo => "Luogo",
            CampoDiProva::Reparto => "Reparto",
            CampoDiProva::Ricovero => "Ricovero",
            CampoDiProva::Allergie => "Allergie",
            CampoDiProva::Note => "Note",
        }
    }

    /// Il nome del campo nel PDF.
    pub fn nome(self) -> &'static str {
        match self {
            CampoDiProva::Sottoscritto => "sottoscritto",
            CampoDiProva::LuogoDiNascita => "nascita.luogo",
            CampoDiProva::DataDiNascita => "nascita.data",
            CampoDiProva::Residente => "residenza",
            CampoDiProva::Acconsente => "consenso.si",
            CampoDiProva::NonAcconsente => "consenso.no",
            CampoDiProva::Data => "firma.data",
            CampoDiProva::Luogo => "firma.luogo",
            CampoDiProva::Reparto => "reparto",
            CampoDiProva::Ricovero => "ricovero",
            CampoDiProva::Allergie => "allergie",
            CampoDiProva::Note => "note",
        }
    }
}

/// Le scelte del menu del reparto, dei radio del ricovero e dell'elenco delle allergie.
pub const REPARTI: [&str; 4] = ["Medicina", "Chirurgia", "Cardiologia", "Ortopedia"];
pub const RICOVERI: [&str; 2] = ["Ordinario", "Day hospital"];
pub const ALLERGIE: [&str; 5] = ["Nessuna", "Farmaci", "Alimenti", "Lattice", "Polline"];

/// Il consenso del bozzetto 7 come modulo vero, coi campi AcroForm aggiunti dal motore sopra le
/// etichette: nella prima pagina «Il/La sottoscritto/a», «nato/a a» e la data di nascita (una data
/// in «dd/mm/yyyy»), «residente in» (vuoto), le due caselle del consenso, la data e il luogo (vuoto);
/// nella seconda il reparto (un menu), il ricovero (due radio), le allergie (un elenco a scelta
/// multipla) e le note (più righe), tutti compilati. Senza bordi né fondo: li evidenzia l'app nel
/// modo Compila. Testo inventato.
pub fn modulo_da_compilare() -> Vec<u8> {
    use pdfcraft_engine::{Edit, FieldLookPatch, FieldProps, FieldValue, NewField, form_scripts::Format};
    use std::sync::Arc;
    let mut f = Foglio::new();
    f.centrato(44.0, 13.0, true, TESTO, "CONSENSO INFORMATO AL TRATTAMENTO");
    f.centrato(61.5, 8.5, false, GRIGIO, "Modulo 2 \u{b7} da compilare a cura del paziente o del tutore");
    let mut posti = Vec::new();
    posti.extend(f.riga_con_posti(88.2, &[("Il/La sottoscritto/a", 0.0, None), ("", 1.0, None)], false));
    posti.extend(f.riga_con_posti(117.2, &[("nato/a a", 0.0, None), ("", 1.0, None), ("il", 0.0, None), ("", 0.6, None)], false));
    posti.extend(f.riga_con_posti(146.2, &[("residente in", 0.0, None), ("", 1.0, None)], false));
    for (i, riga) in [
        "dichiara di essere stato/a informato/a in modo chiaro e comprensibile sulla",
        "natura del trattamento, sui benefici attesi, sui rischi e sulle alternative",
        "possibili, e di aver potuto porre domande ricevendo risposte esaurienti.",
    ]
    .into_iter()
    .enumerate()
    {
        f.testo(28.0, 188.0 + 13.775 * i as f32, 9.5, false, TESTO, riga);
    }
    f.testo(45.0, 228.5 + 10.5, 9.5, false, TESTO, "acconsente al trattamento proposto");
    f.testo(45.0, 248.3 + 10.5, 9.5, false, TESTO, "non acconsente al trattamento proposto");
    posti.push(egui::Rect::from_min_size(egui::pos2(28.0, 228.5), egui::vec2(13.0, 13.0)));
    posti.push(egui::Rect::from_min_size(egui::pos2(28.0, 248.3), egui::vec2(13.0, 13.0)));
    posti.extend(f.riga_con_posti(273.1, &[("Data", 0.0, None), ("", 0.5, None), ("Luogo", 0.0, None), ("", 1.0, None)], false));
    f.riquadro(egui::Rect::from_min_size(egui::pos2(28.0, 309.1), egui::vec2(324.0, 62.0)), 8.0, [0.988, 0.953, 0.945], (1.5, ROSSO), true);
    f.centrato(344.5, 13.0, true, ROSSO, "Firma del paziente o del tutore");
    let prima = f.flusso;
    let mut f = Foglio::new();
    f.centrato(44.0, 13.0, true, TESTO, "DATI DEL RICOVERO");
    f.testo(28.0, 88.2 + 14.0, 9.5, false, TESTO, "Reparto");
    f.testo(28.0, 124.0 + 10.0, 9.5, false, TESTO, "Ricovero");
    for (i, r) in RICOVERI.iter().enumerate() {
        f.testo(118.0 + 100.0 * i as f32, 124.0 + 10.0, 9.5, false, TESTO, r);
    }
    f.testo(28.0, 156.0 + 12.0, 9.5, false, TESTO, "Allergie");
    f.testo(28.0, 240.0 + 12.0, 9.5, false, TESTO, "Note");
    let seconda = f.flusso;
    let pagina = |contenuto| Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto };
    let byte = scrivi(
        &[pagina(&prima), pagina(&seconda), Pagina::bianca(595.0, 842.0)],
        &[],
        &[],
        Some(("Consenso informato al trattamento", "Ambulatorio di prova")),
    );
    // Da px del bozzetto (dall'alto a sinistra) allo spazio utente del PDF.
    let utente =
        |r: egui::Rect| [f64::from(r.left() * PX), f64::from(842.0 - r.bottom() * PX), f64::from(r.right() * PX), f64::from(842.0 - r.top() * PX)];
    let px = |x: f32, y: f32, w: f32, h: f32| utente(egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h)));
    let mut s = pdfcraft_engine::Session::new();
    let id = s.open("prova.pdf", None, Arc::new(byte), None).expect("il modulo di prova si apre");
    let mut applica = |e: Edit| s.apply(id, e).expect("il motore aggiunge il campo");
    use CampoDiProva as C;
    let nuovi: Vec<(C, usize, [f64; 4], NewField)> = vec![
        (C::Sottoscritto, 0, utente(posti[0]), NewField::Text { multiline: false }),
        (C::LuogoDiNascita, 0, utente(posti[1]), NewField::Text { multiline: false }),
        (C::DataDiNascita, 0, utente(posti[2]), NewField::Date),
        (C::Residente, 0, utente(posti[3]), NewField::Text { multiline: false }),
        (C::Acconsente, 0, utente(posti[4]), NewField::CheckBox),
        (C::NonAcconsente, 0, utente(posti[5]), NewField::CheckBox),
        (C::Data, 0, utente(posti[6]), NewField::Date),
        (C::Luogo, 0, utente(posti[7]), NewField::Text { multiline: false }),
        (C::Reparto, 1, px(118.0, 88.2, 160.0, 20.0), NewField::Combo { options: REPARTI.map(String::from).to_vec(), editable: false }),
        (C::Allergie, 1, px(118.0, 156.0, 160.0, 64.0), NewField::List { options: ALLERGIE.map(String::from).to_vec(), multi: true }),
        (C::Note, 1, px(118.0, 240.0, 234.0, 48.0), NewField::Text { multiline: true }),
    ];
    for (c, pagina, rect, kind) in nuovi {
        applica(Edit::AddField { page: pagina, rect, kind, name: Some(c.nome().to_string()) });
    }
    for (i, r) in RICOVERI.iter().enumerate() {
        let kind = NewField::Radio { group: Some(C::Ricovero.nome().to_string()), export: r.to_string() };
        applica(Edit::AddField { page: 1, rect: px(104.0 + 100.0 * i as f32, 124.0, 11.0, 11.0), kind, name: None });
    }
    let senza_bordo = FieldLookPatch { border: Some(None), fill: Some(None), ..Default::default() };
    let grigio = FieldLookPatch { border: Some(Some([0.467, 0.467, 0.467])), fill: Some(None), width: Some(1.0), ..Default::default() };
    for c in CampoDiProva::TUTTI {
        let casella = matches!(c, C::Acconsente | C::NonAcconsente | C::Ricovero);
        let mut props = FieldProps {
            tooltip: Some(c.suggerimento().to_string()),
            appearance: Some(if casella { grigio } else { senza_bordo }),
            ..Default::default()
        };
        // I caratteri del bozzetto (11 px su una pagina larga 380, cioè 17 pt); le note più piccole.
        if matches!(c, C::Sottoscritto | C::LuogoDiNascita | C::DataDiNascita | C::Residente | C::Data | C::Luogo) {
            props.font_size = Some(16.0);
        }
        if c == C::Note {
            props.font_size = Some(11.0);
        }
        if matches!(c, C::DataDiNascita | C::Data) {
            props.format = Some(Format::Date("dd/mm/yyyy".to_string()));
        }
        applica(Edit::SetFieldProps { name: c.nome().to_string(), props: Box::new(props) });
    }
    let valori = [
        (C::Sottoscritto, FieldValue::Text("Paziente A".into())),
        (C::LuogoDiNascita, FieldValue::Text("Roma".into())),
        (C::DataDiNascita, FieldValue::Text("12/03/1958".into())),
        (C::Acconsente, FieldValue::Check(true)),
        (C::Data, FieldValue::Text("24/10/2026".into())),
        (C::Reparto, FieldValue::Choice(vec!["Medicina".into()])),
        (C::Ricovero, FieldValue::Radio(Some(RICOVERI[0].to_string()))),
        (C::Allergie, FieldValue::Choice(vec!["Nessuna".into()])),
        (C::Note, FieldValue::Text("Nessuna nota".into())),
    ];
    for (c, v) in valori {
        applica(Edit::SetFieldValue { name: c.nome().to_string(), value: v });
    }
    // Il campo della firma sul riquadro rosso, senza bordo né fondo: «Tocca per firmare» lo
    // disegna l'app.
    let firma = px(28.0, 309.1, 324.0, 62.0);
    applica(Edit::AddField { page: 0, rect: firma, kind: NewField::Signature, name: Some(CAMPO_FIRMA.to_string()) });
    let props = FieldProps { tooltip: Some("Firma del paziente o del tutore".into()), appearance: Some(senza_bordo), ..Default::default() };
    applica(Edit::SetFieldProps { name: CAMPO_FIRMA.to_string(), props: Box::new(props) });
    s.save_full_bytes(id).expect("il motore salva il modulo").to_vec()
}

/// Il nome del campo della firma del modulo da compilare.
pub const CAMPO_FIRMA: &str = "firma.paziente";

/// La foto di prova (JPEG, `w` × `h`, qualità `qualita`) di una firma su carta: un foglio grigio
/// chiaro con un po' di grana e la luce che cala verso un lato, con una firma blu scura a tratti
/// spessi in mezzo. Niente di vero.
pub fn foto_di_una_firma(w: u32, h: u32, qualita: u8) -> Vec<u8> {
    let mut foto = image::RgbImage::from_fn(w, h, |x, y| {
        let grana = ((x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)) % 13) as u8;
        let luce = 225 - (u64::from(x) * 30 / u64::from(w)) as u8;
        image::Rgb([luce + grana, luce + grana, luce - 5 + grana])
    });
    let (cx, cy, largo) = (w as f32 / 2.0, h as f32 / 2.0, w as f32 * 0.3);
    let spessore = (w.max(h) as f32 / 300.0).max(2.0) as i32;
    let passi = (largo as usize * 2).max(400);
    for i in 0..passi {
        let t = i as f32 / passi as f32;
        let (x, y) = (cx - largo + 2.0 * largo * t, cy + largo * 0.15 * (t * 18.0).sin());
        for dx in -spessore..=spessore {
            for dy in -spessore..=spessore {
                let (px, py) = (x as i32 + dx, y as i32 + dy);
                if px >= 0 && py >= 0 && (px as u32) < w && (py as u32) < h {
                    foto.put_pixel(px as u32, py as u32, image::Rgb([30, 35, 80]));
                }
            }
        }
    }
    let mut v = Vec::new();
    foto.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut v, qualita)).expect("la foto di prova in JPEG");
    v
}

/// Il modulo da compilare con un calcolo vero: uno script che non è una funzione AF di Acrobat
/// sul luogo della firma, che gira solo con gli script accesi.
pub fn modulo_coi_calcoli() -> Vec<u8> {
    use std::sync::Arc;
    let mut s = pdfcraft_engine::Session::new();
    let id = s.open("prova.pdf", None, Arc::new(modulo_da_compilare()), None).expect("il modulo di prova si apre");
    let script = pdfcraft_engine::Edit::SetFieldScript {
        name: CampoDiProva::Luogo.nome().to_string(),
        event: "calculate".into(),
        script: Some("event.value = 'Roma' + 1;".into()),
    };
    s.apply(id, script).expect("il motore mette lo script");
    s.save_bytes(id).expect("il motore salva il modulo").to_vec()
}

/// Le frasi della pagina di prova, con le accentate dell'italiano.
pub const FRASI: [&str; 4] = [
    "perch\\351 per\\362 pi\\371 citt\\340",
    "\\310 gi\\340 cos\\354: la citt\\340 pi\\371 bella",
    "Caff\\350, virt\\371, libert\\340.",
    "Nove giorni fa",
];
pub const PAROLE: [&str; 10] = ["perché", "però", "più", "città", "È", "già", "così", "Caffè", "virtù", "libertà"];

/// Un PDF di una pagina con una foto finta a 320 dpi (1600 × 1600 pixel, 5 pollici): rumore
/// colorato, che si comprime poco senza ridurre le immagini (Comprimi, P6).
pub fn pdf_con_foto() -> Vec<u8> {
    let (w, h) = (1600u32, 1600u32);
    let mut seme = 7u32;
    let px: Vec<u8> = (0..w * h * 3)
        .map(|i| {
            seme = seme.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            ((i % 251) as u8).wrapping_add((seme >> 24) as u8 / 4)
        })
        .collect();
    let mut png = Vec::new();
    image::RgbImage::from_raw(w, h, px).expect("misura").write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).expect("png");
    let s = pdfcraft_engine::Session::new();
    s.create_from_images_with_resolution(&[("foto.png".into(), png)], pdfcraft_engine::ImageResolution::Dpi(320.0)).expect("creato").to_vec()
}

/// Un A4 con le frasi in Helvetica 16 pt.
pub fn pagina_italiana() -> Vec<u8> {
    pagina_con(&FRASI)
}

/// Una pagina A4 con `frasi` (stringhe del PDF) in Helvetica da 16 pt, una riga ogni 24 pt da 72 pt.
fn pagina_con(frasi: &[&str]) -> Vec<u8> {
    let mut c = String::from("BT /F1 16 Tf 72 740 Td 24 TL ");
    for f in frasi {
        c.push_str(&format!("({f}) Tj T* "));
    }
    c.push_str("ET");
    pdf(&[Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: &c }])
}

/// La foto di [`scansione_riconosciuta_su_carta`] se il foglio avesse l'ultima riga scritta
/// `ultima` (lettere ASCII): com'è la scansione di un foglio scritto già giusto.
pub fn foto_su_carta_con_l_ultima_riga(ultima: &str) -> image::RgbImage {
    let frasi = [FRASI[0], FRASI[1], FRASI[2], ultima];
    foto_a_200_dpi(pagina_con(&frasi), CARTA_AVORIO)
}

/// La pagina `pdf` disegnata a 200 dpi su una carta del colore `carta` (il bianco diventa quel
/// colore, il nero resta nero).
fn foto_a_200_dpi(pdf: Vec<u8>, carta: [u8; 3]) -> image::RgbImage {
    let pdf = crate::Pdf::apri(pdf, None).expect("aperto");
    let foto = pdf.disegnatore().disegna(0, 200.0 / 72.0).expect("disegnata");
    let tinta = |v: u8, c: u8| (u16::from(v) * u16::from(c) / 255) as u8;
    let rgb: Vec<u8> = foto.pixels.iter().flat_map(|p| [tinta(p.r(), carta[0]), tinta(p.g(), carta[1]), tinta(p.b(), carta[2])]).collect();
    image::RgbImage::from_raw(foto.size[0] as u32, foto.size[1] as u32, rgb).expect("misura")
}

/// Le frasi della pagina di prova come si leggono.
pub const RIGHE: [&str; 4] = ["perché però più città", "È già così: la città più bella", "Caffè, virtù, libertà.", "Nove giorni fa"];

/// La scansione italiana già riconosciuta: lo strato di testo invisibile di un OCR con le parole
/// delle [`RIGHE`] dove stanno, scritto qui senza i modelli.
pub fn scansione_riconosciuta() -> Vec<u8> {
    con_lo_strato_ocr(scansione_italiana(), |parola| 8.0 * parola.chars().count() as f64)
}

/// La carta color avorio di [`scansione_riconosciuta_su_carta`].
pub const CARTA_AVORIO: [u8; 3] = [245, 237, 219];

/// Com'è una pagina fotografata col telefono e riconosciuta: la pagina italiana in JPEG, su carta
/// color avorio, con lo strato dell'OCR; ogni parola larga quanto le sue lettere in Helvetica, come
/// la legge un riconoscitore vero.
pub fn scansione_riconosciuta_su_carta() -> Vec<u8> {
    let largo = |parola: &str| pdfcraft_engine::added_text_width(&pdfcraft_engine::AddedText { text: parola.into(), size: 16.0, ..Default::default() });
    con_lo_strato_ocr(scansione_in(image::ImageFormat::Jpeg, CARTA_AVORIO), largo)
}

/// `byte` (la pagina italiana come foto) con lo strato invisibile delle parole delle [`RIGHE`]:
/// larghe `largo`, da 72 pt, una riga ogni 24 pt come nella pagina italiana.
fn con_lo_strato_ocr(byte: Vec<u8>, largo: impl Fn(&str) -> f64) -> Vec<u8> {
    use std::sync::Arc;
    let mut s = pdfcraft_engine::Session::new();
    let id = s.open("scansione.pdf", None, Arc::new(byte), None).expect("la scansione si apre");
    let mut words = Vec::new();
    for (i, riga) in RIGHE.iter().enumerate() {
        let base = 740.0 - 24.0 * i as f64;
        let mut x = 72.0;
        for parola in riga.split(' ') {
            let largo = largo(parola);
            words.push(pdfcraft_engine::ocr::PlacedWord { text: parola.into(), origin: [x, base - 4.0], across: [largo, 0.0], up: [0.0, 16.0] });
            x += largo + 4.5;
        }
    }
    s.apply(id, pdfcraft_engine::Edit::AddOcrText { page: 0, words }).expect("lo strato di testo");
    s.save_full_bytes(id).expect("salvata").to_vec()
}

/// La pagina come la vedrebbe uno scanner: disegnata a 200 dpi e rimessa in un PDF fatto
/// solo dell'immagine, senza testo.
pub fn scansione_italiana() -> Vec<u8> {
    scansione_in(image::ImageFormat::Png, [255, 255, 255])
}

/// La pagina italiana disegnata a 200 dpi su una carta del colore `carta` (il bianco diventa
/// quel colore, il nero resta nero), in `formato` (PNG o JPEG), in un PDF fatto solo della foto.
fn scansione_in(formato: image::ImageFormat, carta: [u8; 3]) -> Vec<u8> {
    let img = foto_a_200_dpi(pagina_italiana(), carta);
    let mut byte = Vec::new();
    let nome = match formato {
        image::ImageFormat::Jpeg => {
            img.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut byte, 88)).expect("jpeg");
            "pagina.jpg"
        }
        _ => {
            img.write_to(&mut std::io::Cursor::new(&mut byte), image::ImageFormat::Png).expect("png");
            "pagina.png"
        }
    };
    let s = pdfcraft_engine::Session::new();
    let byte = s.create_from_images_with_resolution(&[(nome.into(), byte)], pdfcraft_engine::ImageResolution::Dpi(200.0)).expect("creato");
    byte.to_vec()
}

/// La foto finta di un foglio: la pagina italiana disegnata a 150 dpi, posata su un tavolo
/// scuro con un margine, in JPEG.
pub fn foto_di_un_foglio() -> Vec<u8> {
    let pdf = crate::Pdf::apri(pagina_italiana(), None).expect("aperto");
    let foglio = pdf.disegnatore().disegna(0, 150.0 / 72.0).expect("disegnata");
    let (fw, fh) = (foglio.size[0] as u32, foglio.size[1] as u32);
    let (w, h) = (fw + 260, fh + 300);
    let mut tavolo = image::RgbImage::from_pixel(w, h, image::Rgb([52, 44, 38]));
    for y in 0..fh {
        for x in 0..fw {
            let p = foglio.pixels[(y * fw + x) as usize];
            // Un po' d'ombra da sinistra, come una lampada di lato.
            let ombra = 1.0 - 0.18 * (1.0 - x as f32 / fw as f32);
            let c = |v: u8| (v as f32 * ombra) as u8;
            tavolo.put_pixel(x + 120, y + 140, image::Rgb([c(p.r()), c(p.g()), c(p.b())]));
        }
    }
    let mut jpeg = Vec::new();
    tavolo.write_with_encoder(image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88)).expect("jpeg");
    jpeg
}

/// Un PDF scritto a mano: gli oggetti numerati da 1 nell'ordine, il catalogo è il primo.
pub fn pdf_grezzo(oggetti: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut posti = Vec::new();
    for (i, o) in oggetti.iter().enumerate() {
        posti.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).bytes());
        out.extend(o.as_bytes());
        out.extend(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", oggetti.len() + 1).bytes());
    for p in posti {
        out.extend(format!("{p:010} 00000 n \n").bytes());
    }
    out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", oggetti.len() + 1).bytes());
    out
}

/// Un flusso col suo `/Length`, per [`pdf_grezzo`].
pub fn flusso(dizionario: &str, contenuto: &str) -> String {
    format!("<< {dizionario} /Length {} >>\nstream\n{contenuto}\nendstream", contenuto.len())
}

/// Un modulo con due caselle da 10 punti a 4 punti l'una dall'altra, «Si» e «No» (come quelle di
/// un'anamnesi o di una scala del dolore): senza zoom sono più piccole del dito.
pub fn caselle_vicine() -> Vec<u8> {
    let acceso = flusso("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", "0 g 2 2 6 6 re f 0 G 0.5 0.5 9 9 re S");
    let spento = flusso("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", "0 G 0.5 0.5 9 9 re S");
    let contenuto = "BT /F1 11 Tf 40 702 Td (Fumatore:) Tj ET BT /F1 9 Tf 101 690 Td (Si) Tj ET BT /F1 9 Tf 115 690 Td (No) Tj ET";
    pdf_grezzo(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [6 0 R 7 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> /Annots [6 0 R 7 0 R] >>".into(),
        flusso("", contenuto),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
        "<< /FT /Btn /T (fumatore_si) /TU (Si) /V /Off /Type /Annot /Subtype /Widget /P 3 0 R /Rect [100 700 110 710] /F 4 /AS /Off /AP << /N << /Yes 8 0 R /Off 9 0 R >> >> >>".into(),
        "<< /FT /Btn /T (fumatore_no) /TU (No) /V /Off /Type /Annot /Subtype /Widget /P 3 0 R /Rect [114 700 124 710] /F 4 /AS /Off /AP << /N << /Yes 8 0 R /Off 9 0 R >> >> >>".into(),
        acceso,
        spento,
    ])
}

/// Un modulo con una casella a due posti che si escludono, «Uno» e «Due»: lo stesso campo con due
/// stati accesi diversi, come le fa Acrobat.
pub fn casella_a_due_stati() -> Vec<u8> {
    let acceso = flusso("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", "0 g 2 2 6 6 re f 0 G 0.5 0.5 9 9 re S");
    let spento = flusso("/Type /XObject /Subtype /Form /BBox [0 0 10 10]", "0 G 0.5 0.5 9 9 re S");
    pdf_grezzo(&[
        "<< /Type /Catalog /Pages 2 0 R /AcroForm << /Fields [6 0 R] /DA (/Helv 0 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >> >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> /Annots [7 0 R 8 0 R] >>".into(),
        flusso("", "BT /F1 11 Tf 40 602 Td (Scelta:) Tj ET"),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".into(),
        "<< /FT /Btn /T (scelta) /TU (Scelta) /V /Off /Kids [7 0 R 8 0 R] >>".into(),
        "<< /Type /Annot /Subtype /Widget /Parent 6 0 R /P 3 0 R /Rect [100 600 112 612] /F 4 /AS /Off /AP << /N << /Uno 9 0 R /Off 10 0 R >> >> >>"
            .into(),
        "<< /Type /Annot /Subtype /Widget /Parent 6 0 R /P 3 0 R /Rect [200 600 212 612] /F 4 /AS /Off /AP << /N << /Due 9 0 R /Off 10 0 R >> >> >>"
            .into(),
        acceso,
        spento,
    ])
}

#[cfg(test)]
mod test {
    use super::*;

    /// Le misure del piano: apertura e disegno di una pagina sul PC, col modulo e con un PDF di
    /// 300 pagine fitte. A mano: `cargo test -p tascabile-pdf misure -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn misure_sul_pc() {
        use std::time::Instant;
        let fitto: String = (0..60)
            .map(|i| format!("BT /F1 10 Tf 40 {} Td (Riga {i} di una pagina fitta, con tanto testo per il motore di disegno.) Tj ET\n", 800 - 13 * i))
            .collect::<String>()
            + "0 0 1 RG 0.5 w 30 30 m 565 812 l 30 812 m 565 30 l S";
        let tante = pdf(&vec![Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: &fitto }; 300]);
        // La scala del telefono: 380 dp per 595 pt a 2,625 pixel per dp.
        let scala = 380.0 / 595.0 * 2.625;
        for (nome, byte) in [("modulo di 3 pagine", modulo()), ("300 pagine fitte", tante)] {
            let misura = byte.len();
            let t = Instant::now();
            let p = crate::Pdf::apri(byte, None).unwrap();
            let apertura = t.elapsed();
            let t = Instant::now();
            let mut d = p.disegnatore();
            let disegnatore = t.elapsed();
            let t = Instant::now();
            let img = d.disegna(0, scala).unwrap();
            let pagina = t.elapsed();
            let t = Instant::now();
            let r = crate::Rettangolo { x: 1024, y: 1024, larghezza: 1024, altezza: 1024 };
            d.disegna_rettangolo(0, scala * 3.0, r).unwrap();
            let tessera = t.elapsed();
            // Il testo di tutte le pagine e la ricerca senza accenti, come la fa il lettore.
            let t = Instant::now();
            let mut motore = pdfcraft_render::PageRenderer::new(p.documento().display.clone(), Default::default());
            let mut ricerca = crate::Ricerca::new("TRATTAMENTO", p.pagine(), 0).unwrap();
            let mut testi = Vec::new();
            for i in 0..p.pagine() {
                let testo = motore
                    .render(pdfcraft_render::RenderRequest { page: i, kind: pdfcraft_render::RequestKind::Text, ..Default::default() })
                    .text
                    .unwrap();
                ricerca.leggi(i, &testo);
                testi.push(testo);
            }
            let cerca = t.elapsed();
            // Un tasto dopo, coi testi già letti: rinormalizzandoli (come la 0.3.0) e con la forma
            // del confronto tenuta (il lettore di adesso).
            let t = Instant::now();
            let mut r = crate::Ricerca::new("trattamento m", p.pagine(), 0).unwrap();
            for (i, testo) in testi.iter().enumerate() {
                r.leggi(i, testo);
            }
            let rinormalizzando = t.elapsed();
            let normali: Vec<_> = testi.iter().map(|t| crate::ricerca::TestoNormale::new(t)).collect();
            let t = Instant::now();
            let mut r = crate::Ricerca::new("trattamento m", p.pagine(), 0).unwrap();
            for (i, testo) in testi.iter().enumerate() {
                r.leggi_normale(i, testo, &normali[i]);
            }
            let tenuta = t.elapsed();
            eprintln!(
                "{nome}: {misura} byte, {} pagine, apertura {apertura:.1?}, disegnatore {disegnatore:.1?}, pagina {}×{} {pagina:.1?}, tessera a 3 volte {tessera:.1?}, testo e ricerca di tutte le pagine su un thread {cerca:.1?} ({} risultati); un tasto dopo {rinormalizzando:.1?} rinormalizzando, {tenuta:.1?} con la forma tenuta",
                p.pagine(),
                img.size[0],
                img.size[1],
                ricerca.totale()
            );
        }
    }

    #[test]
    fn il_modulo_si_apre_e_ha_testo_e_colori() {
        let p = crate::Pdf::apri(modulo(), None).unwrap();
        assert_eq!(p.pagine(), 3);
        let img = p.disegnatore().disegna(0, 1.0).unwrap();
        let conta = |f: &dyn Fn(egui::Color32) -> bool| img.pixels.iter().filter(|c| f(**c)).count();
        // Testo scuro, campi blu, firma rossa: c'è tutto.
        assert!(conta(&|c| c.r() < 90 && c.g() < 90 && c.b() < 90) > 2_000);
        assert!(conta(&|c| c.b() > 140 && c.r() < 80) > 300);
        assert!(conta(&|c| c.r() > 170 && c.g() < 100 && c.b() < 90) > 300);
        assert_eq!(stringa("Modulo \u{b7} è (a)"), "(Modulo \\267 \\350 \\(a\\))");
    }
}
