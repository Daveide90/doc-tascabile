//! Ponte tastiera: tiene allineati il testo che ha la tastiera di Android (la finestra)
//! e il documento di WordCraft. La tastiera lavora in UTF-16, il motore in byte UTF-8.
//!
//! A ogni stato della tastiera: differenza con lo specchio, comandi del motore, poi il
//! controllo che il documento dica esattamente quello che ha la tastiera; solo dopo il
//! cursore va dove lo vuole la tastiera. Se il motore ha fatto altro (campi, elenchi),
//! il cursore resta dove l'ha messo il motore e la tastiera riceve una finestra nuova.

use serde_json::{Value, json};
use tascabile_platform::ime::{Modifica, byte_to_utf16, differenza, utf16_to_byte};
use tascabile_platform::{ImeSnapshot, TastoCrudo, diagnosi};
use wordcraft_engine::doc::{InlineObject, Pos};
use wordcraft_engine::{CmdError, Selection, Session};

use crate::finestra::{A_CAPO_DI_RIGA, Finestra, LIMITE_UTF16, MAX_UTF16, RAGGIO, SEGNAPOSTO};

/// Testo e selezione da dare alla tastiera (selezione in unità UTF-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatoIme {
    pub testo: String,
    pub sel: (i32, i32),
    pub versione: i64,
}

#[derive(Debug, Default)]
pub struct Ponte {
    finestra: Option<Finestra>,
    /// Il testo che la tastiera ha in questo momento.
    specchio: String,
    /// La selezione che la tastiera ha in questo momento (byte dello specchio).
    sel_specchio: (usize, usize),
    /// Versione dell'ultimo stato dato alla tastiera.
    versione: i64,
    /// Parola in composizione, da sottolineare.
    pub composizione: Option<(Pos, Pos)>,
    /// Perché la tastiera non può scrivere qui (paragrafo troppo lungo), da mostrare.
    pub avviso: Option<String>,
    /// La selezione del documento esce dalla finestra (Seleziona tutto, una maniglia trascinata
    /// per più paragrafi): la tastiera ha solo un cursore, ma sullo schermo c'è la selezione, e
    /// come su Android quello che si batte la sostituisce.
    sel_fuori: bool,
}

/// Com'è andato uno stato della tastiera, per la diagnosi.
enum Esito {
    /// Costruito su un testo più vecchio dell'ultimo dato alla tastiera: scartato.
    Vecchio,
    /// La tastiera non è collegata al documento (nessuna finestra): ignorato.
    Scollegato,
    Applicato,
    /// Applicato, ma il documento non dice quello che ha la tastiera: finestra nuova.
    NonTornano,
    /// Applicato, poi la finestra si è ricentrata (cursore sul bordo, finestra troppo grande).
    Ricentrato,
}

/// Genere di un errore del motore, per la diagnosi: il messaggio può riportare i parametri del
/// comando (anche il testo da inserire), il genere no.
pub(crate) fn genere(e: &CmdError) -> &'static str {
    match e {
        CmdError::Unknown(_) => "sconosciuto",
        CmdError::Params(_) => "parametri sbagliati",
        CmdError::Disabled(_) => "non disponibile",
        CmdError::Failed(_) => "fallito",
    }
}

/// «finestra nuova v8», o «tastiera scollegata» se la finestra nuova non si è potuta fare.
fn finestra_nuova(st: &Option<StatoIme>) -> String {
    st.as_ref().map_or_else(|| "tastiera scollegata".to_string(), |st| format!("finestra nuova v{}", st.versione))
}

fn esegui(s: &mut Session, id: &str, p: Value) -> Result<(), String> {
    s.run(id, &p).map(|_| ()).map_err(|e| {
        diagnosi(format!("ponte: {id} non riuscito ({})", genere(&e)));
        format!("{id}: {e}")
    })
}

/// Testo della tastiera → testo per il motore: U+000B torna a capo di riga; U+2060 (i nostri
/// segnaposto) non si inserisce mai come testo.
fn per_il_motore(t: &str) -> String {
    t.replace(A_CAPO_DI_RIGA, "\n").replace(SEGNAPOSTO, "")
}

/// Mette il cursore in `pos` solo se non c'è già (ogni spostamento chiude il gruppo di Annulla).
fn cursore_in(s: &mut Session, pos: &Pos) -> Result<(), String> {
    if s.sel.ordered() == (pos.clone(), pos.clone()) {
        return Ok(());
    }
    esegui(s, "caret.set", json!({ "pos": pos }))
}

/// Inserisce `testo` della tastiera al cursore del motore: ogni a capo è un paragrafo nuovo.
fn inserisci_al_cursore(s: &mut Session, testo: &str) -> Result<(), String> {
    let testo = testo.replace("\r\n", "\n").replace(['\r', '\u{2029}'], "\n");
    for (i, pezzo) in testo.split('\n').enumerate() {
        if i > 0 {
            esegui(s, "text.newParagraph", json!({}))?;
        }
        let pezzo = per_il_motore(pezzo);
        if !pezzo.is_empty() {
            esegui(s, "text.insert", json!({ "text": pezzo, "raw": true }))?;
        }
    }
    Ok(())
}

/// Una cancellazione della tastiera dentro il passo di Annulla della scrittura, se è aperto (come
/// `delete_surrounding` di `ui-egui/src/keys.rs`): la parola battuta e le sue correzioni sono
/// un passo solo. Dopo, il passo si riapre per le lettere che seguono (`continue_typing`, che lo
/// fa solo se l'ultimo passo è della scrittura).
fn cancella_nel_passo(s: &mut Session, unisci: bool) -> Result<(), String> {
    if unisci {
        s.join_next_undo();
    }
    esegui(s, "text.backspace", json!({}))?;
    if unisci {
        s.continue_typing();
    }
    Ok(())
}

/// All'inizio di una parola nuova il passo di Annulla della scrittura si chiude: ogni parola è
/// un passo, e Annulla toglie solo l'ultima. Il motore chiuderebbe il passo solo spostando il
/// cursore. Una parola nuova è una lettera battuta dopo uno spazio, o un inserimento che comincia
/// con lo spazio e ha una parola dentro: la scrittura a scorrimento di Gboard e SwiftKey manda
/// « mondo» tutto insieme (analisi del 27/10, word I7: un Annulla toglieva tutta la frase).
fn parola_nuova(s: &mut Session, testo: &str) {
    if !s.typing_open() {
        return;
    }
    let mut caratteri = testo.chars();
    let Some(primo) = caratteri.next() else { return };
    if primo.is_whitespace() {
        if caratteri.any(|c| !c.is_whitespace()) {
            s.close_typing();
        }
        return;
    }
    let c = s.sel.focus.clone();
    let prima = s.doc.para_at(&c).and_then(|p| p.text.get(..c.off)).and_then(|t| t.chars().next_back());
    if prima.is_some_and(char::is_whitespace) {
        s.close_typing();
    }
}

/// L'oggetto del testo in `pos` è un campo o il richiamo di una nota, che un Backspace sceglie
/// prima di cancellare.
fn da_scegliere_prima(s: &Session, pos: &Pos) -> bool {
    let Some(p) = s.doc.para_at(pos) else { return false };
    let Some(prima) = p.text.get(..pos.off) else { return false };
    let indice = prima.matches('\u{FFFC}').count();
    matches!(p.objects.get(indice), Some(InlineObject::Field { .. } | InlineObject::NoteRef { .. }))
}

/// Applica una modifica della finestra al documento, con i comandi di WordCraft.
/// `prima` è la selezione che la tastiera aveva (byte del testo vecchio).
fn applica(s: &mut Session, f: &Finestra, vecchio: &str, prima: (usize, usize), m: &Modifica) -> Result<(), String> {
    // Backspace subito dopo segnaposto invisibili (la fine di un commento, un segnalibro: U+2060
    // nella finestra): la tastiera cancella il segnaposto, e il commento perderebbe la fine o il
    // titolo dell'indice il suo segnalibro. Si cancella invece quello che c'è prima di loro, col
    // tasto del motore; i segnaposto restano, e la finestra nuova la dà il controllo dopo, perché
    // il testo non torna con quello della tastiera.
    if m.inserisci.is_empty() && prima == (m.a, m.a) && m.da < m.a && vecchio[m.da..m.a].chars().all(|c| c == SEGNAPOSTO) {
        let mut inizio = m.da;
        while let Some(c) = vecchio[..inizio].chars().next_back()
            && c == SEGNAPOSTO
        {
            inizio -= c.len_utf8();
        }
        let prima_dei_segni = vecchio[..inizio].chars().next_back();
        cursore_in(s, &f.pos(vecchio, inizio))?;
        // Una lettera resta nel passo della parola; l'unione di due paragrafi no.
        let unisci = s.typing_open() && prima_dei_segni.is_some_and(|c| c != '\n');
        return cancella_nel_passo(s, unisci);
    }
    // Backspace subito dopo un campo (una data, un numero di pagina, un'unione) o il richiamo di
    // una nota: per la tastiera sono un carattere solo, e il tasto li toglieva interi (analisi del
    // 27/10, word I1). Come in Word il primo Backspace li sceglie, il secondo cancella la scelta.
    // La finestra nuova la dà il controllo dopo, perché il testo non torna con quello della
    // tastiera.
    if m.inserisci.is_empty() && prima == (m.a, m.a) && &vecchio[m.da..m.a] == "\u{FFFC}" {
        let (da, a) = (f.pos(vecchio, m.da), f.pos(vecchio, m.a));
        if da_scegliere_prima(s, &da) {
            return esegui(s, "select.range", json!({ "anchor": da, "focus": a }));
        }
    }
    // Backspace della tastiera (cancella proprio quello che cancellerebbe il tasto del motore):
    // si usa il tasto del motore, così Annulla rimette un cursore e non una selezione, e
    // all'inizio di una voce di elenco toglie il punto come in Word.
    if m.inserisci.is_empty() && prima == (m.a, m.a) {
        let fine = f.pos(vecchio, m.a);
        let uguale = &vecchio[m.da..m.a] == "\n"
            || s.doc.para_at(&fine).is_some_and(|p| fine.off >= m.a - m.da && p.backspace_from(fine.off) == fine.off - (m.a - m.da));
        if uguale {
            cursore_in(s, &fine)?;
            // Dentro un paragrafo resta nel passo della parola; l'unione di due paragrafi no.
            let unisci = s.typing_open() && &vecchio[m.da..m.a] != "\n";
            return cancella_nel_passo(s, unisci);
        }
    }
    // Un incolla da Windows porta "\r\n": per il motore ogni a capo è un paragrafo nuovo.
    let inserisci = m.inserisci.replace("\r\n", "\n").replace(['\r', '\u{2029}'], "\n");
    let mut pezzi = inserisci.split('\n');
    let primo = per_il_motore(pezzi.next().unwrap_or(""));
    // Una correzione della tastiera dentro un paragrafo, col passo della scrittura aperto: la
    // parte da cambiare si seleziona direttamente, senza `select.range` (un comando di selezione
    // che sposta il cursore chiuderebbe il passo), e il testo nuovo la sostituisce nello stesso
    // passo. `text.insert` sopra una selezione prende il formato del primo carattere scelto.
    let in_un_paragrafo = !vecchio[m.da..m.a].contains('\n') && !inserisci.contains('\n');
    if m.da != m.a && in_un_paragrafo && s.typing_open() {
        let (da, a) = (f.pos(vecchio, m.da), f.pos(vecchio, m.a));
        s.sel = Selection { anchor: da, focus: a };
        return if primo.is_empty() { cancella_nel_passo(s, true) } else { esegui(s, "text.insert", json!({ "text": primo, "raw": true })) };
    }
    if m.da != m.a {
        let (da, a) = (f.pos(vecchio, m.da), f.pos(vecchio, m.a));
        esegui(s, "select.range", json!({ "anchor": da, "focus": a }))?;
        if primo.is_empty() {
            esegui(s, "text.backspace", json!({}))?; // con una selezione cancella solo quella
        } else {
            esegui(s, "text.insert", json!({ "text": primo, "raw": true }))?;
        }
    } else {
        cursore_in(s, &f.pos(vecchio, m.da))?;
        if !primo.is_empty() {
            parola_nuova(s, &primo);
            esegui(s, "text.insert", json!({ "text": primo, "raw": true }))?;
        }
    }
    for pezzo in pezzi {
        // In WordCraft "\n" dentro text.insert è un a capo di riga: il paragrafo nuovo è un comando.
        esegui(s, "text.newParagraph", json!({}))?;
        let pezzo = per_il_motore(pezzo);
        if !pezzo.is_empty() {
            esegui(s, "text.insert", json!({ "text": pezzo, "raw": true }))?;
        }
    }
    Ok(())
}

impl Ponte {
    pub fn attivo(&self) -> bool {
        self.finestra.is_some()
    }

    /// Un documento nuovo riprende le versioni da quello di prima: uno stato in ritardo del
    /// documento vecchio non deve passare per buono nel nuovo.
    pub fn continua_da(&mut self, prima: &Ponte) {
        self.versione = self.versione.max(prima.versione);
    }

    /// La versione dell'ultimo stato dato alla tastiera.
    pub fn versione(&self) -> i64 {
        self.versione
    }

    /// Come `continua_da`, da una versione data alla tastiera da altri (il campo di Trova): uno
    /// stato in ritardo del campo non deve passare per buono nel documento.
    pub fn continua_dalla_versione(&mut self, versione: i64) {
        self.versione = self.versione.max(versione);
    }

    pub fn chiudi(&mut self) {
        self.finestra = None;
        self.composizione = None;
    }

    /// La finestra più grande (raggio da RAGGIO a 0) che sta in MAX_UTF16, col suo testo.
    fn costruisci(s: &Session) -> Option<(Finestra, String)> {
        let mut raggio = RAGGIO;
        loop {
            let f = Finestra::attorno(s, raggio)?;
            let testo = f.testo(s)?;
            if raggio == 0 || testo.encode_utf16().count() <= MAX_UTF16 {
                return Some((f, testo));
            }
            raggio -= 1;
        }
    }

    /// Finestra nuova attorno al cursore: è lo stato da dare alla tastiera. None se il cursore
    /// non è in un paragrafo, o se anche il solo paragrafo del cursore supera `LIMITE_UTF16`
    /// (in quel caso `avviso` spiega perché); in tutti e due i casi il ponte si chiude.
    pub fn apri(&mut self, s: &Session) -> Option<StatoIme> {
        let Some((f, testo)) = Self::costruisci(s) else {
            diagnosi("ponte: cursore fuori da un paragrafo, tastiera scollegata");
            self.chiudi();
            self.avviso = None; // un vecchio «paragrafo troppo lungo» qui non vale più
            return None;
        };
        let unita = testo.encode_utf16().count();
        if unita > LIMITE_UTF16 {
            diagnosi(format!("ponte: paragrafo di {unita} unità, oltre il limite di {LIMITE_UTF16}: tastiera scollegata"));
            self.chiudi();
            self.avviso = Some("Paragrafo troppo lungo per la tastiera: dividilo in paragrafi più corti.".to_string());
            return None;
        }
        self.avviso = None;
        let (a, b) = s.sel.ordered();
        let fuoco = f.byte(&testo, &s.sel.focus).unwrap_or(0);
        let ba = f.byte(&testo, &a).unwrap_or(fuoco);
        let bb = f.byte(&testo, &b).unwrap_or(fuoco);
        self.versione += 1;
        self.composizione = None;
        self.sel_fuori = a != b && (f.byte(&testo, &a).is_none() || f.byte(&testo, &b).is_none());
        self.sel_specchio = (ba, bb);
        let stato =
            StatoIme { sel: (byte_to_utf16(&testo, ba) as i32, byte_to_utf16(&testo, bb) as i32), testo: testo.clone(), versione: self.versione };
        self.specchio = testo;
        self.finestra = Some(f);
        Some(stato)
    }

    /// Applica uno stato mandato dalla tastiera. Restituisce uno stato da rimandarle quando la
    /// finestra va ricentrata o il documento non torna con la tastiera. Ogni stato lascia una
    /// riga nella diagnosi: versione, misure e com'è andato, mai il testo.
    pub fn ricevi(&mut self, s: &mut Session, snap: &ImeSnapshot) -> Result<Option<StatoIme>, String> {
        let esito = self.applica_stato(s, snap);
        let com_e_andato = match &esito {
            Ok((_, Esito::Vecchio)) => format!("scartato perché vecchio (ultimo invio v{})", self.versione),
            Ok((_, Esito::Scollegato)) => "ignorato: tastiera scollegata dal documento".to_string(),
            Ok((_, Esito::Applicato)) => "applicato".to_string(),
            Ok((st, Esito::NonTornano)) => format!("documento e tastiera non tornano, {}", finestra_nuova(st)),
            Ok((st, Esito::Ricentrato)) => format!("applicato, {}", finestra_nuova(st)),
            Err(_) => "errore del motore".to_string(),
        };
        let comp = if snap.comp.0 >= 0 && snap.comp.1 > snap.comp.0 { format!(", comp {}..{}", snap.comp.0, snap.comp.1) } else { String::new() };
        diagnosi(format!(
            "tastiera: stato v{}, {} unità, sel {}..{}{comp}, {com_e_andato}",
            snap.version,
            snap.text.encode_utf16().count(),
            snap.sel.0,
            snap.sel.1
        ));
        esito.map(|(st, _)| st)
    }

    fn applica_stato(&mut self, s: &mut Session, snap: &ImeSnapshot) -> Result<(Option<StatoIme>, Esito), String> {
        if snap.version < self.versione {
            return Ok((None, Esito::Vecchio)); // costruito sul testo di prima del nostro ultimo invio
        }
        let Some(f) = self.finestra.clone() else {
            return Ok((None, Esito::Scollegato));
        };
        let nuovo = snap.text.as_str();
        let x = utf16_to_byte(nuovo, snap.sel.0.max(0) as usize);
        let y = utf16_to_byte(nuovo, snap.sel.1.max(0) as usize);
        let (sel_a, sel_b) = (x.min(y), x.max(y));
        if self.sel_fuori && s.sel.anchor != s.sel.focus {
            if let Some(m) = differenza(&self.specchio, nuovo, sel_b) {
                // Prima via la selezione del documento, poi solo quello che la tastiera ha
                // aggiunto (niente per un Backspace), poi una finestra nuova. Sopra delle celle
                // intere una lettera le svuota (`text.delete`, come in Word): il Backspace del
                // motore toglierebbe le righe o le colonne scelte tutte.
                self.sel_fuori = false;
                let id = if s.cell_range().is_some() && !m.inserisci.is_empty() { "text.delete" } else { "text.backspace" };
                esegui(s, id, json!({}))?;
                inserisci_al_cursore(s, &m.inserisci)?;
                return Ok((self.apri(s), Esito::Ricentrato));
            }
            if (sel_a, sel_b) == self.sel_specchio {
                return Ok((None, Esito::Applicato));
            }
        }
        self.sel_fuori = false;
        if let Some(m) = differenza(&self.specchio, nuovo, sel_b) {
            applica(s, &f, &self.specchio, self.sel_specchio, &m)?;
        }
        let f2 = Finestra { n: nuovo.matches('\n').count() as u32 + 1, ..f };
        // Prima il controllo: il documento deve dire esattamente quello che ha la tastiera.
        // Se no, il cursore resta dove l'ha messo il motore e la tastiera riceve una finestra nuova.
        if f2.testo(s).as_deref() != Some(nuovo) {
            log::warn!("ponte: documento e tastiera non tornano, finestra nuova");
            return Ok((self.apri(s), Esito::NonTornano));
        }
        // Poi la selezione della tastiera e la parola in composizione.
        let (pa, pb) = (f2.pos(nuovo, sel_a), f2.pos(nuovo, sel_b));
        if sel_a == sel_b {
            cursore_in(s, &pb)?;
        } else if s.sel.ordered() != (pa.clone(), pb.clone()) {
            esegui(s, "select.range", json!({ "anchor": pa, "focus": pb }))?;
        }
        self.composizione = (snap.comp.0 >= 0 && snap.comp.1 > snap.comp.0).then(|| {
            let c0 = utf16_to_byte(nuovo, snap.comp.0 as usize);
            let c1 = utf16_to_byte(nuovo, snap.comp.1 as usize);
            (f2.pos(nuovo, c0), f2.pos(nuovo, c1))
        });
        self.specchio = nuovo.to_string();
        self.sel_specchio = (sel_a, sel_b);
        self.finestra = Some(f2.clone());
        // Si ricentra se il cursore è sul bordo con un vicino fuori, o se la finestra supera
        // `MAX_UTF16`; mai una finestra di un solo paragrafo (ricaricherebbe la tastiera a ogni
        // tasto), mai con una selezione aperta (la tastiera la perderebbe, il motore no), mai per
        // il numero di paragrafi (sul telefono una ricarica dopo Invio può perdere lettere).
        let lunghezza = nuovo.encode_utf16().count();
        let k = f2.paragrafo_di(nuovo, sel_b);
        let sul_bordo = sel_a == sel_b && f2.n > 1 && ((k == 0 && f2.ha_prima(s)) || (k + 1 == f2.n && f2.ha_dopo(s)));
        let troppo_grande = f2.n > 1 && lunghezza > MAX_UTF16;
        if sul_bordo || troppo_grande || lunghezza > LIMITE_UTF16 {
            return Ok((self.apri(s), Esito::Ricentrato));
        }
        Ok((None, Esito::Applicato))
    }

    /// Un tasto che la tastiera ha lasciato all'app (Backspace col cursore a inizio finestra,
    /// Canc col cursore alla fine, Invio dove si trova il cursore del motore).
    pub fn tasto_crudo(&mut self, s: &mut Session, t: TastoCrudo) -> Result<Option<StatoIme>, String> {
        // GameTextInput lascia passare Backspace solo col SUO cursore all'inizio della finestra e
        // Canc solo alla fine: il cursore del motore va lì prima del tasto. Invio non dice dove sia
        // il cursore della tastiera (col ponte attivo GameTextInput non lo lascia mai passare): il
        // motore resta al suo cursore.
        let (id, bordo) = match t {
            TastoCrudo::Backspace => ("text.backspace", Some(0)),
            TastoCrudo::Canc => ("text.delete", Some(self.specchio.len())),
            TastoCrudo::Invio => ("text.newParagraph", None),
        };
        // Il tasto crudo vale solo se la tastiera, per quello che sappiamo, ha il cursore proprio su
        // quel bordo della finestra che ha adesso: se la finestra è cambiata nel frattempo (stato
        // nuovo non ancora arrivato alla tastiera) si perde un Backspace o un Canc, ma non si
        // uniscono i paragrafi sbagliati. Lo stesso se la tastiera ha spostato il cursore senza
        // avvisare.
        // Con la selezione fuori dalla finestra Backspace e Canc la cancellano.
        if self.sel_fuori && bordo.is_some() && s.sel.anchor != s.sel.focus {
            self.sel_fuori = false;
            esegui(s, "text.backspace", json!({}))?;
            let st = self.apri(s);
            diagnosi(format!("tastiera: tasto {t:?} lasciato all'app, selezione cancellata, {}", finestra_nuova(&st)));
            return Ok(st);
        }
        if let Some(b) = bordo
            && self.finestra.is_some()
            && self.sel_specchio != (b, b)
        {
            diagnosi(format!("tastiera: tasto {t:?} lasciato all'app, perso: la tastiera non ha il cursore sul bordo"));
            return Ok(None);
        }
        if let (Some(f), Some(b)) = (self.finestra.clone(), bordo) {
            cursore_in(s, &f.pos(&self.specchio, b))?;
        }
        esegui(s, id, json!({}))?;
        let attivo = self.attivo();
        let st = if attivo { self.apri(s) } else { None };
        let finestra = if attivo { format!(", {}", finestra_nuova(&st)) } else { String::new() };
        diagnosi(format!("tastiera: tasto {t:?} lasciato all'app, applicato{finestra}"));
        Ok(st)
    }

    /// Dopo un comando che non viene dalla tastiera (formato, annulla): se testo o cursore non
    /// sono più quelli che ha la tastiera, le si dà una finestra nuova.
    pub fn dopo_comando(&mut self, s: &Session) -> Option<StatoIme> {
        let f = self.finestra.as_ref()?;
        let uguale = f.testo(s).as_deref() == Some(self.specchio.as_str());
        let (a, b) = s.sel.ordered();
        let sel = (f.byte(&self.specchio, &a), f.byte(&self.specchio, &b));
        if uguale && sel == (Some(self.sel_specchio.0), Some(self.sel_specchio.1)) {
            return None;
        }
        self.apri(s)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::{documento, testi};
    use tascabile_platform::Piattaforma;
    use wordcraft_engine::doc::CharProps;

    /// La tastiera finta collegata al ponte, come l'app su Android ma tutta in memoria.
    struct Banco {
        p: Piattaforma,
        ponte: Ponte,
        s: Session,
    }

    impl Banco {
        fn new(paragrafi: &[&str]) -> Banco {
            Banco { p: Piattaforma::default(), ponte: Ponte::default(), s: documento(paragrafi) }
        }

        fn cursore(&mut self, blocco: usize, off: usize) {
            self.s.run("caret.set", &json!({ "pos": Pos::body(blocco, off) })).unwrap();
            let st = self.ponte.apri(&self.s).unwrap();
            self.manda(st);
            self.p.show_keyboard();
        }

        fn manda(&self, st: StatoIme) {
            self.p.set_ime_state(st.testo, st.sel.0, st.sel.1, st.versione);
        }

        /// Consegna al ponte l'ultimo stato della tastiera finta.
        fn giro(&mut self) {
            if let Some(snap) = self.p.take_ime_snapshot()
                && let Some(st) = self.ponte.ricevi(&mut self.s, &snap).unwrap()
            {
                self.manda(st);
            }
        }

        fn scrivi(&mut self, t: &str) {
            self.p.simula_testo(t);
            self.giro();
        }

        /// Come `passo_logica` dell'app: prima l'ultimo stato della tastiera, poi i tasti che ha
        /// lasciato all'app nello stesso passo.
        fn passo(&mut self) {
            // Fuori da un passo di egui `tasti_crudi` non applica eventi: dà solo i tasti in coda.
            let crudi = self.p.tasti_crudi(&egui::Context::default());
            self.giro();
            for t in crudi {
                if let Some(st) = self.ponte.tasto_crudo(&mut self.s, t).unwrap() {
                    self.manda(st);
                }
            }
        }
    }

    #[test]
    fn scrivere_alla_fine_del_paragrafo() {
        let mut b = Banco::new(&["Ciao", "mondo"]);
        b.cursore(0, 4);
        b.scrivi(" a tutti");
        assert_eq!(testi(&b.s), ["Ciao a tutti", "mondo"]);
        assert_eq!(b.s.sel.focus, Pos::body(0, 12));
    }

    #[test]
    fn lettera_per_lettera_con_accenti() {
        let mut b = Banco::new(&[""]);
        b.cursore(0, 0);
        for c in ["P", "e", "r", "c", "h", "é", " ", "è"] {
            b.scrivi(c);
        }
        assert_eq!(testi(&b.s), ["Perché è"]);
    }

    #[test]
    fn invio_crea_un_paragrafo() {
        let mut b = Banco::new(&["Ciao", "mondo"]);
        b.cursore(0, 4);
        b.scrivi("\n");
        b.scrivi("nuovo");
        assert_eq!(testi(&b.s), ["Ciao", "nuovo", "mondo"]);
        assert_eq!(b.s.sel.focus, Pos::body(1, 5));
    }

    #[test]
    fn backspace_unisce_i_paragrafi() {
        let mut b = Banco::new(&["Ciao", "mondo"]);
        b.cursore(1, 0);
        b.p.simula_backspace();
        b.giro();
        assert_eq!(testi(&b.s), ["Ciaomondo"]);
        assert_eq!(b.s.sel.focus, Pos::body(0, 4));
    }

    #[test]
    fn autocorrezione_sostituisce_la_parola() {
        let mut b = Banco::new(&["Ho visto teh"]);
        b.cursore(0, 12);
        b.p.simula_sostituzione(9, 12, "the ");
        b.giro();
        assert_eq!(testi(&b.s), ["Ho visto the "]);
    }

    /// Annulla finché si può, col testo del primo paragrafo dopo ogni passo.
    fn annulla_tutto(b: &mut Banco) -> Vec<String> {
        let mut visti = Vec::new();
        while b.s.can_undo() {
            b.s.run("edit.undo", &json!({})).unwrap();
            visti.push(testi(&b.s)[0].clone());
        }
        visti
    }

    #[test]
    fn annulla_toglie_una_parola_alla_volta() {
        let mut b = Banco::new(&[""]);
        b.cursore(0, 0);
        b.s.reset_history();
        for c in "Ciao mondo bello".chars() {
            b.scrivi(&c.to_string());
        }
        assert_eq!(testi(&b.s), ["Ciao mondo bello"]);
        // Ogni parola con gli spazi dopo è un passo, come le parole dei suggerimenti.
        assert_eq!(annulla_tutto(&mut b), ["Ciao mondo ", "Ciao ", ""]);
    }

    #[test]
    fn il_primo_backspace_dopo_un_campo_lo_sceglie_il_secondo_lo_cancella() {
        // Un campo DATE di Word in fondo a un verbale: per la tastiera è un carattere solo, e un
        // Backspace toglieva tutta la data (analisi del 27/10, word I1). Come in Word: il primo
        // Backspace sceglie il campo, il secondo lo cancella.
        let corpo = r#"<w:p><w:r><w:t xml:space="preserve">Roma, </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> DATE \@ "dd/MM/yyyy" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>26/10/2026</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p><w:p><w:r><w:t>Firma del medico</w:t></w:r></w:p>"#;
        let s = crate::documento::apri_byte("verbale.docx", &crate::prova::docx(corpo, "", &[])).unwrap();
        let campi = |s: &Session| s.doc.body[0].as_para().unwrap().objects.iter().filter(|o| matches!(o, InlineObject::Field { .. })).count();
        assert_eq!(campi(&s), 1, "il campo non è un oggetto solo");
        let fine = s.doc.body[0].as_para().unwrap().text.len();
        let mut b = Banco { p: Piattaforma::default(), ponte: Ponte::default(), s };
        b.cursore(0, fine);
        b.p.simula_backspace();
        b.giro();
        assert_eq!(campi(&b.s), 1, "il primo Backspace ha tolto il campo");
        assert!(!b.s.sel.is_collapsed(), "il campo non è scelto");
        b.p.simula_backspace();
        b.giro();
        assert_eq!(campi(&b.s), 0, "il secondo Backspace non l'ha tolto");
        assert_eq!(testi(&b.s)[0], "Roma, ");
    }

    #[test]
    fn con_la_scrittura_a_scorrimento_annulla_toglie_una_parola_alla_volta() {
        // Gboard e SwiftKey, scrivendo col dito che scorre, mandano lo spazio davanti alla parola
        // nuova: « mondo», poi « bello». Un Annulla toglieva tutta la frase (analisi del 27/10,
        // word I7).
        let mut b = Banco::new(&[""]);
        b.cursore(0, 0);
        b.s.reset_history();
        for parola in ["Ciao", " mondo", " bello"] {
            b.scrivi(parola);
        }
        assert_eq!(testi(&b.s), ["Ciao mondo bello"]);
        assert_eq!(annulla_tutto(&mut b), ["Ciao mondo", "Ciao", ""]);
    }

    #[test]
    fn la_correzione_della_tastiera_sta_nel_passo_della_parola() {
        let mut b = Banco::new(&[""]);
        b.cursore(0, 0);
        b.s.reset_history();
        for c in "Ho visto teh".chars() {
            b.scrivi(&c.to_string());
        }
        // Lo spazio fa correggere «teh» in «the »: un solo stato, che sostituisce la parola.
        b.p.simula_sostituzione(9, 12, "the ");
        b.giro();
        b.scrivi("x");
        assert_eq!(testi(&b.s), ["Ho visto the x"]);
        // Un solo Annulla toglie la parola con la sua correzione: «teh» non torna.
        assert_eq!(annulla_tutto(&mut b), ["Ho visto the ", "Ho visto ", "Ho ", ""]);
    }

    #[test]
    fn backspace_dentro_la_parola_resta_nel_suo_passo() {
        let mut b = Banco::new(&["Ciao"]);
        b.cursore(0, 4);
        b.s.reset_history();
        for c in " gatto".chars() {
            b.scrivi(&c.to_string());
        }
        for _ in 0..2 {
            b.p.simula_backspace();
            b.giro();
        }
        b.scrivi("ta");
        assert_eq!(testi(&b.s), ["Ciao gatta"]);
        assert_eq!(annulla_tutto(&mut b), ["Ciao ", "Ciao"]);
    }

    #[test]
    fn dopo_un_tocco_la_correzione_e_un_passo_suo() {
        // Il cursore messo col dito chiude il passo della scrittura: un suggerimento scelto dopo,
        // che sostituisce la parola, è un passo nuovo, e le lettere che seguono vanno con lui.
        let mut b = Banco::new(&["Ciao mondo"]);
        b.cursore(0, 10);
        b.s.reset_history();
        b.p.simula_sostituzione(5, 10, "mondi");
        b.giro();
        b.scrivi("x");
        assert_eq!(testi(&b.s), ["Ciao mondix"]);
        assert_eq!(annulla_tutto(&mut b), ["Ciao mondo"]);
    }

    #[test]
    fn la_correzione_tiene_il_formato_della_parola() {
        let mut b = Banco::new(&["Ciao "]);
        b.cursore(0, 5);
        b.s.run("format.bold", &json!({})).unwrap();
        for c in "teh".chars() {
            b.scrivi(&c.to_string());
        }
        b.p.simula_sostituzione(5, 8, "the ");
        b.giro();
        let p = b.s.doc.para_at(&Pos::body(0, 0)).unwrap();
        let grassetto = |off: usize| p.props_of_char(off).bold == Some(true);
        assert!(!grassetto(0) && grassetto(5) && grassetto(7), "il grassetto della parola corretta");
        assert_eq!(testi(&b.s), ["Ciao the "]);
    }

    #[test]
    fn emoji_nei_byte_giusti() {
        let mut b = Banco::new(&["ab"]);
        b.cursore(0, 1);
        b.scrivi("😀");
        assert_eq!(testi(&b.s), ["a😀b"]);
        assert_eq!(b.s.sel.focus, Pos::body(0, 5));
    }

    #[test]
    fn freccia_sposta_il_cursore_del_documento() {
        let mut b = Banco::new(&["città"]);
        b.cursore(0, 6);
        b.p.simula_freccia(false);
        b.giro();
        assert_eq!(b.s.sel.focus, Pos::body(0, 4));
    }

    #[test]
    fn stato_vecchio_scartato() {
        let mut b = Banco::new(&["abc"]);
        b.cursore(0, 3);
        b.p.simula_testo("x");
        let vecchio = b.p.take_ime_snapshot().unwrap();
        let _ = b.ponte.apri(&b.s); // l'app ha già mandato un testo più nuovo
        assert_eq!(b.ponte.ricevi(&mut b.s, &vecchio).unwrap(), None);
        assert_eq!(testi(&b.s), ["abc"]);
    }

    #[test]
    fn annulla_rimanda_la_finestra() {
        let mut b = Banco::new(&["abc"]);
        b.cursore(0, 3);
        b.scrivi("def");
        b.s.run("edit.undo", &json!({})).unwrap();
        let st = b.ponte.dopo_comando(&b.s).expect("la tastiera deve ricevere il testo nuovo");
        assert_eq!(st.testo, testi(&b.s)[0]);
    }

    #[test]
    fn grassetto_non_disturba_la_tastiera() {
        let mut b = Banco::new(&["abc"]);
        b.cursore(0, 3);
        b.s.run("format.bold", &json!({})).unwrap();
        assert_eq!(b.ponte.dopo_comando(&b.s), None);
    }

    #[test]
    fn tanti_invii_senza_ricaricare_la_tastiera() {
        let mut b = Banco::new(&["p0", "p1", "p2", "p3", "p4", "p5", "p6"]);
        b.cursore(3, 2);
        let versione = b.ponte.versione;
        for _ in 0..6 {
            b.scrivi("\n");
        }
        assert_eq!(b.s.doc.body.len(), 13);
        // nessuna finestra nuova: sul telefono una ricarica dopo Invio può perdere le lettere battute
        assert_eq!(b.ponte.versione, versione);
        assert_eq!(b.s.sel.focus, Pos::body(9, 0));
    }

    #[test]
    fn backspace_crudo_all_inizio_del_documento() {
        let mut b = Banco::new(&["abc"]);
        b.cursore(0, 0);
        let st = b.ponte.tasto_crudo(&mut b.s, TastoCrudo::Backspace).unwrap();
        assert!(st.is_some());
        assert_eq!(testi(&b.s), ["abc"]);
    }

    #[test]
    fn dopo_una_citazione_il_testo_resta_in_ordine() {
        // Campo complesso (citazione di Zotero): il motore può mettere il testo battuto in un punto
        // diverso da dove lo crede la tastiera. Il ponte controlla prima di spostare il cursore.
        let mut b = Banco::new(&["Vedi Rossi 2020 qui"]);
        let p = CharProps::default();
        b.s.doc.insert_object(&Pos::body(0, 15), InlineObject::FieldEnd, &p).unwrap();
        b.s.doc
            .insert_object(&Pos::body(0, 5), InlineObject::FieldStart { instr: "ADDIN ZOTERO_ITEM".into(), locked: false, dirty: false }, &p)
            .unwrap();
        b.cursore(0, 18); // subito dopo la citazione: il cursore cade prima della fine del campo
        for c in ["X", "Y", "Z"] {
            b.scrivi(c);
        }
        let t = testi(&b.s)[0].replace('\u{FFFC}', "|");
        assert!(t.contains("XYZ"), "{t:?}");
    }

    #[test]
    fn invio_su_voce_vuota_chiude_l_elenco_senza_spostare_il_testo() {
        let mut b = Banco::new(&["uno", "fine"]);
        b.s.run("caret.set", &json!({ "pos": Pos::body(0, 3) })).unwrap();
        b.s.run("para.bullets", &json!({})).unwrap();
        b.cursore(0, 3);
        b.scrivi("\n"); // voce nuova, vuota
        b.scrivi("\n"); // Invio sulla voce vuota: come in Word chiude l'elenco, niente paragrafo nuovo
        for c in ["d", "u", "e"] {
            b.scrivi(c);
        }
        assert_eq!(testi(&b.s), ["uno", "due", "fine"]);
    }

    #[test]
    fn paragrafo_oltre_il_limite_senza_tastiera() {
        let enorme = "parola ".repeat(2000); // 14000 unità UTF-16
        let mut s = documento(&[enorme.as_str()]);
        s.run("caret.set", &json!({ "pos": Pos::body(0, 0) })).unwrap();
        let mut ponte = Ponte::default();
        assert_eq!(ponte.apri(&s), None);
        assert!(ponte.avviso.is_some());
        assert!(!ponte.attivo());
    }

    #[test]
    fn paragrafo_lungo_fra_vicini_non_ricarica_la_tastiera_a_ogni_tasto() {
        let lungo = "parola ".repeat(1300); // 9100 unità: oltre MAX_UTF16, sotto LIMITE_UTF16
        let mut b = Banco::new(&["prima", lungo.as_str(), "dopo"]);
        b.cursore(1, 0);
        let versione = b.ponte.versione;
        b.scrivi("x");
        assert_eq!(b.ponte.versione, versione, "nessuna finestra nuova a ogni tasto");
        assert!(testi(&b.s)[1].starts_with("xparola"));
    }

    #[test]
    fn backspace_dopo_un_segno_invisibile_cancella_la_lettera_e_tiene_il_segno() {
        // Analisi del 27/10, critico 7: col cursore dopo la fine di un segnalibro (o di un
        // commento) il primo Backspace della tastiera toglieva il segno invisibile, e il commento
        // o il titolo dell'indice si rompevano.
        let mut b = Banco::new(&["Ciao mondo"]);
        b.s.run("select.range", &json!({ "anchor": Pos::body(0, 5), "focus": Pos::body(0, 10) })).unwrap();
        b.s.run("insert.bookmark", &json!({ "name": "Segno" })).unwrap();
        let segni = |s: &Session| s.doc.body[0].as_para().unwrap().objects.iter().filter(|o| o.is_marker()).count();
        let visibile = |s: &Session| testi(s).iter().map(|t| t.replace('\u{FFFC}', "")).collect::<Vec<_>>();
        assert_eq!(segni(&b.s), 2);
        let fine = b.s.doc.body[0].as_para().unwrap().text.len();
        b.cursore(0, fine);
        b.p.simula_backspace();
        b.giro();
        assert_eq!(segni(&b.s), 2, "il Backspace ha tolto il segno invisibile");
        assert_eq!(visibile(&b.s), ["Ciao mond"]);
        // Il secondo cancella la lettera prima, sempre senza toccare il segno.
        b.p.simula_backspace();
        b.giro();
        assert_eq!((segni(&b.s), visibile(&b.s)), (2, vec!["Ciao mon".to_string()]));
        // Lo stesso dopo la fine di un commento.
        let mut b = Banco::new(&["Ciao mondo"]);
        b.s.run("select.range", &json!({ "anchor": Pos::body(0, 5), "focus": Pos::body(0, 10) })).unwrap();
        b.s.run("review.newComment", &json!({ "text": "Quale mondo?" })).unwrap();
        let (prima, fine) = (segni(&b.s), b.s.doc.body[0].as_para().unwrap().text.len());
        assert!(prima > 0);
        b.cursore(0, fine);
        b.p.simula_backspace();
        b.giro();
        assert_eq!((segni(&b.s), visibile(&b.s)), (prima, vec!["Ciao mond".to_string()]), "il commento ha perso la fine");
    }

    #[test]
    fn segnaposto_dalla_tastiera_non_entra_nel_documento() {
        let mut b = Banco::new(&["ab"]);
        b.cursore(0, 1);
        b.scrivi("x\u{2060}y");
        assert_eq!(testi(&b.s), ["axyb"]);
    }

    /// Il testo che si legge (senza quello tolto con le revisioni).
    fn come_si_legge(s: &Session) -> Vec<String> {
        s.doc.body.iter().filter_map(|b| b.as_para()).map(|p| crate::salva::come_si_legge(&s.doc, p)).collect()
    }

    #[test]
    fn con_le_revisioni_la_tastiera_scrive_e_cancella() {
        let mut b = Banco::new(&["Ciao mondo"]);
        b.s.run("review.trackChanges", &json!({ "value": true })).unwrap();
        b.cursore(0, 10);
        // «mondo» cancellato lettera per lettera col Backspace, poi «tutti» scritto.
        for _ in 0..5 {
            b.p.simula_backspace();
            b.giro();
        }
        b.scrivi("tutti");
        assert_eq!(come_si_legge(&b.s), ["Ciao tutti"]);
        // Le revisioni: «mondo» tolto, «tutti» aggiunto; rifiutate tutte, torna «Ciao mondo».
        let st = crate::revisioni::stato(&mut b.s);
        let generi: Vec<_> = st.modifiche.iter().map(|m| m.genere).collect();
        assert!(
            generi.contains(&crate::revisioni::GenereModifica::Tolto) && generi.contains(&crate::revisioni::GenereModifica::Aggiunto),
            "{generi:?}"
        );
        b.s.run("review.rejectAll", &json!({})).unwrap();
        assert_eq!(come_si_legge(&b.s), ["Ciao mondo"]);
    }

    #[test]
    fn backspace_poi_annulla_lascia_un_cursore() {
        let mut b = Banco::new(&["Ciao"]);
        b.cursore(0, 4);
        b.p.simula_backspace();
        b.giro();
        assert_eq!(testi(&b.s), ["Cia"]);
        b.s.run("edit.undo", &json!({})).unwrap();
        assert_eq!(testi(&b.s), ["Ciao"]);
        assert_eq!(b.s.sel.ordered(), (Pos::body(0, 4), Pos::body(0, 4)));
    }

    #[test]
    fn backspace_all_inizio_di_una_voce_toglie_il_punto() {
        let mut b = Banco::new(&["uno", "due"]);
        b.s.run("caret.set", &json!({ "pos": Pos::body(1, 0) })).unwrap();
        b.s.run("para.bullets", &json!({})).unwrap();
        b.cursore(1, 0);
        b.p.simula_backspace();
        b.giro();
        // come in Word: il primo Backspace toglie il punto, il testo resta nel suo paragrafo
        assert_eq!(testi(&b.s), ["uno", "due"]);
    }

    #[test]
    fn selezione_dalla_tastiera_seleziona_nel_documento() {
        let mut b = Banco::new(&["Ciao mondo"]);
        b.cursore(0, 10);
        let snap = ImeSnapshot { text: "Ciao mondo".into(), sel: (5, 10), comp: (-1, -1), version: b.ponte.versione };
        assert_eq!(b.ponte.ricevi(&mut b.s, &snap).unwrap(), None);
        assert_eq!(b.s.sel.ordered(), (Pos::body(0, 5), Pos::body(0, 10)));
    }

    #[test]
    fn stato_uguale_non_ricarica_la_tastiera() {
        let mut b = Banco::new(&["p0", "p1", "p2"]);
        b.cursore(1, 1);
        let versione = b.ponte.versione;
        let (testo, inizio, fine, v) = b.p.ultimo_stato().unwrap();
        let snap = ImeSnapshot { text: testo, sel: (inizio, fine), comp: (-1, -1), version: v };
        assert_eq!(b.ponte.ricevi(&mut b.s, &snap).unwrap(), None);
        assert_eq!(b.ponte.versione, versione);
    }

    #[test]
    fn composizione_sottolineata() {
        let mut b = Banco::new(&["Ciao mondo"]);
        b.cursore(0, 10);
        let snap = ImeSnapshot { text: "Ciao mondo".into(), sel: (10, 10), comp: (5, 10), version: b.ponte.versione };
        b.ponte.ricevi(&mut b.s, &snap).unwrap();
        assert_eq!(b.ponte.composizione, Some((Pos::body(0, 5), Pos::body(0, 10))));
    }

    #[test]
    fn seleziona_tutto_non_ricentra() {
        let mut b = Banco::new(&["p0", "p1", "p2", "p3", "p4", "p5", "p6"]);
        b.cursore(3, 1);
        let (testo, _, _, v) = b.p.ultimo_stato().unwrap();
        assert_eq!(testo, "p1\np2\np3\np4\np5");
        let snap = ImeSnapshot { text: testo.clone(), sel: (0, testo.len() as i32), comp: (-1, -1), version: v };
        assert_eq!(b.ponte.ricevi(&mut b.s, &snap).unwrap(), None);
        assert_eq!(b.s.sel.ordered(), (Pos::body(1, 0), Pos::body(5, 2)));
    }

    #[test]
    fn backspace_crudo_col_cursore_lontano_dal_bordo_si_perde() {
        // La tastiera ha spostato il SUO cursore all'inizio senza avvisare: per lo specchio è
        // ancora in fondo, e il tasto potrebbe essere per una finestra che non c'è più. Si perde
        // un Backspace, il caso peggiore accettato: niente cancellato, niente paragrafi uniti,
        // cursore dov'era.
        let mut b = Banco::new(&["Titolo", "Ciao mondo"]);
        b.cursore(1, 10);
        assert_eq!(b.ponte.tasto_crudo(&mut b.s, TastoCrudo::Backspace).unwrap(), None);
        assert_eq!(testi(&b.s), ["Titolo", "Ciao mondo"]);
        assert_eq!(b.s.sel.focus, Pos::body(1, 10));
    }

    #[test]
    fn tre_backspace_sulle_righe_vuote_non_uniscono_paragrafi_lontani() {
        // Tre Backspace nello stesso passo col cursore su «c», dopo due righe vuote: i primi due le
        // tolgono nella tastiera, e lo stato che arriva fa ricentrare la finestra; il terzo trova il
        // cursore della tastiera all'inizio della finestra vecchia e arriva all'app come tasto crudo.
        let mut b = Banco::new(&["q0", "q1", "p0", "p1", "", "", "c", "p5", "p6", "p7"]);
        b.cursore(6, 0);
        for _ in 0..3 {
            b.p.simula_backspace();
        }
        b.passo();
        // Nella finestra nuova il cursore non è sul bordo: il tasto si perde. All'inizio della
        // finestra nuova unirebbe «q1» e «p0», lontano dal cursore.
        assert_eq!(testi(&b.s), ["q0", "q1", "p0", "p1", "c", "p5", "p6", "p7"]);
        assert_eq!(b.s.sel.focus, Pos::body(4, 0));
        b.scrivi("Z");
        assert_eq!(testi(&b.s), ["q0", "q1", "p0", "p1", "Zc", "p5", "p6", "p7"]);
    }

    #[test]
    fn backspace_crudo_e_lettera_nello_stesso_passo() {
        // Il Backspace col cursore della tastiera a 0 va all'app, poi arriva la «x». Lo stato con
        // la «x» passa per primo, e il tasto crudo non riporta il cursore prima della «x».
        let mut b = Banco::new(&[""]);
        b.cursore(0, 0);
        b.p.simula_backspace();
        b.p.simula_testo("x");
        b.passo();
        assert_eq!(testi(&b.s), ["x"]);
        assert_eq!(b.s.sel.focus, Pos::body(0, 1));
        b.scrivi("y");
        assert_eq!(testi(&b.s), ["xy"]);
    }

    #[test]
    fn tasti_crudi_sul_bordo_uniscono_col_paragrafo_fuori_dalla_finestra() {
        // Insieme i due paragrafi superano MAX_UTF16: la finestra ne ha uno solo. Col cursore della
        // tastiera sul bordo, Backspace e Canc arrivano all'app e uniscono i paragrafi.
        let (a, z) = ("a".repeat(4500), "z".repeat(4000));
        let unito = format!("{a}{z}");
        let mut b = Banco::new(&[a.as_str(), z.as_str()]);
        b.cursore(1, 0);
        b.p.simula_backspace();
        b.passo();
        assert_eq!(testi(&b.s), [unito.as_str()]);
        let mut b = Banco::new(&[a.as_str(), z.as_str()]);
        b.cursore(0, 4500);
        b.p.simula_canc();
        b.passo();
        assert_eq!(testi(&b.s), [unito.as_str()]);
    }

    #[test]
    fn incolla_con_a_capo_di_windows() {
        let mut b = Banco::new(&["ab"]);
        b.cursore(0, 1);
        b.scrivi("x\r\ny");
        assert_eq!(testi(&b.s), ["ax", "yb"]);
    }

    #[test]
    fn invio_crudo_resta_al_cursore() {
        let mut b = Banco::new(&["p0", "ab", "p2"]);
        b.cursore(1, 1);
        b.ponte.tasto_crudo(&mut b.s, TastoCrudo::Invio).unwrap();
        assert_eq!(testi(&b.s), ["p0", "a", "b", "p2"]);
    }

    #[test]
    fn diagnosi_senza_il_testo_del_documento_ne_quello_battuto() {
        // Parole che si riconoscono: nessuna deve finire nel registro, né intera né spezzata.
        let mut b = Banco::new(&["Zanzibar è lontana", "Timbuctù"]);
        b.cursore(0, 8);
        for c in ["K", "i", "l", "i", "m"] {
            b.scrivi(c);
        }
        b.scrivi("angiaro e Kilimangiaro\n");
        b.p.simula_backspace();
        b.passo();
        b.p.simula_sostituzione(0, 3, "Ottomila");
        b.passo();
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains(", applicato\n"), "{d}");
        for parola in ["Zanzibar", "lontana", "Timbuctù", "Kilim", "angiaro", "Ottomila"] {
            assert!(!d.contains(parola), "«{parola}» nella diagnosi:\n{d}");
        }
    }

    #[test]
    fn diagnosi_dice_com_e_andato_ogni_stato() {
        // Finestra di 26 unità: «due\ntre\nquattro\ncinque\nsei».
        let mut b = Banco::new(&["uno", "due", "tre", "quattro", "cinque", "sei", "sette"]);
        b.cursore(3, 0);
        // Cinque frecce a sinistra: il cursore della tastiera arriva in fondo a «due», sul bordo
        // della finestra con «uno» fuori, e la finestra si ricentra.
        for _ in 0..5 {
            b.p.simula_freccia(false);
            b.giro();
        }
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] tastiera: stato v1, 26 unità, sel 7..7, applicato\n"), "{d}");
        assert!(d.contains("] tastiera: stato v1, 26 unità, sel 3..3, applicato, finestra nuova v2\n"), "{d}");
        // Uno stato costruito sulla finestra di prima arriva dopo quella nuova.
        let vecchio = ImeSnapshot { text: "due\ntre\nquattro\ncinque\nseix".into(), sel: (27, 27), comp: (-1, -1), version: 1 };
        assert_eq!(b.ponte.ricevi(&mut b.s, &vecchio).unwrap(), None);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] tastiera: stato v1, 27 unità, sel 27..27, scartato perché vecchio (ultimo invio v2)\n"), "{d}");
    }

    #[test]
    fn diagnosi_dei_tasti_crudi() {
        let mut b = Banco::new(&["Tre", "Quattro", "Cinque"]);
        b.cursore(0, 0);
        // Canc vale solo col cursore della tastiera in fondo alla finestra: qui è all'inizio.
        assert_eq!(b.ponte.tasto_crudo(&mut b.s, TastoCrudo::Canc).unwrap(), None);
        // Backspace all'inizio del documento: il motore non cancella niente, la finestra si rifà.
        assert!(b.ponte.tasto_crudo(&mut b.s, TastoCrudo::Backspace).unwrap().is_some());
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains("] tastiera: tasto Canc lasciato all'app, perso: la tastiera non ha il cursore sul bordo\n"), "{d}");
        assert!(d.contains("] tastiera: tasto Backspace lasciato all'app, applicato, finestra nuova v2\n"), "{d}");
    }

    #[test]
    fn diagnosi_quando_la_tastiera_si_scollega() {
        let enorme = "parola ".repeat(1911); // 13377 unità: misura insolita, per ritrovare la riga
        let mut s = documento(&[enorme.as_str()]);
        s.run("caret.set", &json!({ "pos": Pos::body(0, 0) })).unwrap();
        assert_eq!(Ponte::default().apri(&s), None);
        let d = tascabile_platform::diagnosi_testo();
        assert!(d.contains(&format!("] ponte: paragrafo di 13377 unità, oltre il limite di {LIMITE_UTF16}: tastiera scollegata\n")), "{d}");
    }

    #[test]
    fn finestra_impossibile_toglie_il_vecchio_avviso() {
        let enorme = "parola ".repeat(2000);
        let mut s = documento(&[enorme.as_str()]);
        s.run("caret.set", &json!({ "pos": Pos::body(0, 0) })).unwrap();
        let mut ponte = Ponte::default();
        assert_eq!(ponte.apri(&s), None);
        assert!(ponte.avviso.is_some());
        // cursore fuori da qualunque paragrafo: niente finestra, e niente avviso vecchio
        s.sel.focus = Pos::body(99, 0);
        s.sel.anchor = Pos::body(99, 0);
        assert_eq!(ponte.apri(&s), None);
        assert_eq!(ponte.avviso, None);
        assert!(!ponte.attivo());
    }

    /// Sei paragrafi, il cursore su «sei», poi Seleziona tutto: la selezione esce dalla finestra.
    fn tutto_selezionato() -> Banco {
        let mut b = Banco::new(&["uno", "due", "tre", "quattro", "cinque", "sei"]);
        b.cursore(5, 3);
        b.s.run("select.all", &json!({})).unwrap();
        if let Some(st) = b.ponte.dopo_comando(&b.s) {
            b.manda(st);
        }
        b
    }

    #[test]
    fn con_tutto_selezionato_una_lettera_sostituisce_la_selezione() {
        let mut b = tutto_selezionato();
        // Lo stato che la tastiera rimanda uguale non chiude la selezione.
        b.giro();
        assert!(b.s.sel.anchor != b.s.sel.focus, "la selezione si è chiusa");
        b.scrivi("x");
        assert_eq!(testi(&b.s), ["x"]);
        b.scrivi("y");
        assert_eq!(testi(&b.s), ["xy"]);
    }

    #[test]
    fn con_tutto_selezionato_backspace_cancella_la_selezione() {
        let mut b = tutto_selezionato();
        b.p.simula_backspace();
        b.passo();
        assert_eq!(testi(&b.s), [""]);
        b.scrivi("a");
        assert_eq!(testi(&b.s), ["a"]);
    }
}
