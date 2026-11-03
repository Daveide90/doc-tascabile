//! Le modifiche di un PDF viste da Tascabile: che cosa cambiano per le firme digitali, quali
//! pagine toccano (per ridisegnarle) e come le firme già presenti le permettono.
//!
//! Il motore applica qualunque modifica anche su un PDF firmato (rifiuta solo le riscritture
//! complete) e dà il verdetto sulle firme soltanto dopo l'`apply` (ricerca PDFCraft, sezione 9 e
//! verifica). Qui c'è il classificatore di `sign` portato prima della modifica: le firme di
//! approvazione permettono moduli e commenti; una certificazione permette i moduli col permesso 2
//! o 3, i commenti solo col 3, e niente col permesso 1 («nessuna modifica»); pagine, contenuto e
//! link mai. Dopo l'`apply` il motore del thread controlla lo stesso le firme (`motore.rs`).

use pdfcraft_engine::{Edit, SignatureInfo, SignatureStatus};

/// Che cosa cambia una modifica, per le firme digitali, dal più permesso al meno.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GenereModifica {
    /// Titolo, autore e le altre informazioni del documento: sempre permesse.
    Metadati,
    /// Il valore dei campi di un modulo.
    Moduli,
    /// Commenti e segni di Fill & Sign (note, evidenziazioni, penna, firme disegnate).
    Commenti,
    /// Tutto il resto: le pagine (ruotare, togliere, spostare), il loro contenuto, i link.
    Pagine,
}

impl GenereModifica {
    /// Il genere di una modifica del motore; quelle che qui non si conoscono valgono `Pagine`,
    /// il genere che le firme non permettono mai.
    pub fn di(edit: &Edit) -> GenereModifica {
        match edit {
            Edit::SetInfo { .. } => GenereModifica::Metadati,
            Edit::SetFieldValue { .. } | Edit::ResetForm { .. } | Edit::SetFieldImage { .. } => GenereModifica::Moduli,
            Edit::AddAnnotation(a) if !matches!(a.shape, pdfcraft_engine::Shape::Attachment { .. }) => GenereModifica::Commenti,
            Edit::AddCustomStamp { .. }
            | Edit::DeleteAnnotation { .. }
            | Edit::SetAnnotationContents { .. }
            | Edit::ReplyToAnnotation { .. }
            | Edit::SetAnnotationStatus { .. }
            | Edit::EraseInk { .. }
            | Edit::MarkAnnotation { .. }
            | Edit::LockAnnotation { .. }
            | Edit::MoveAnnotation { .. }
            | Edit::ReshapeAnnotation { .. }
            | Edit::ResizeAnnotation { .. }
            | Edit::StyleAnnotation { .. }
            | Edit::FillAnnotation { .. }
            | Edit::FontSizeAnnotation { .. }
            | Edit::SetAnnotationInfo { .. } => GenereModifica::Commenti,
            Edit::Batch { edits, .. } => edits.iter().map(GenereModifica::di).max().unwrap_or(GenereModifica::Metadati),
            _ => GenereModifica::Pagine,
        }
    }

    /// Per la diagnosi.
    pub fn nome(self) -> &'static str {
        match self {
            GenereModifica::Metadati => "informazioni",
            GenereModifica::Moduli => "modulo",
            GenereModifica::Commenti => "commenti",
            GenereModifica::Pagine => "pagine",
        }
    }
}

/// Le pagine che una modifica cambia, da ridisegnare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Toccate {
    /// Nessuna: le informazioni del documento, un salvataggio.
    Nessuna,
    /// Queste pagine (da 0), in ordine e senza doppi.
    Alcune(Vec<usize>),
    /// Tutte, o non si sa quali.
    Tutte,
}

impl Toccate {
    /// Le pagine toccate da una modifica del motore; quelle che qui non si conoscono le toccano
    /// tutte.
    pub fn di(edit: &Edit) -> Toccate {
        match edit {
            Edit::SetInfo { .. } => Toccate::Nessuna,
            Edit::RotatePages { pages, .. } => Toccate::alcune(pages.iter().copied()),
            Edit::AddAnnotation(a) => Toccate::alcune([a.page]),
            Edit::AddCustomStamp { page, .. }
            | Edit::DeleteAnnotation { page, .. }
            | Edit::MoveAnnotation { page, .. }
            | Edit::ResizeAnnotation { page, .. }
            | Edit::FontSizeAnnotation { page, .. }
            | Edit::SetAnnotationContents { page, .. }
            | Edit::EraseInk { page, .. }
            | Edit::StyleAnnotation { page, .. }
            | Edit::ReshapeAnnotation { page, .. }
            | Edit::FillAnnotation { page, .. }
            | Edit::SetAnnotationInfo { page, .. }
            | Edit::AddLink { page, .. }
            | Edit::SetLink { page, .. }
            | Edit::DeleteLink { page, .. }
            | Edit::AddOcrText { page, .. } => Toccate::alcune([*page]),
            Edit::Batch { edits, .. } => edits.iter().map(Toccate::di).fold(Toccate::Nessuna, Toccate::unisci),
            _ => Toccate::Tutte,
        }
    }

    pub(crate) fn alcune(pagine: impl IntoIterator<Item = usize>) -> Toccate {
        let mut v: Vec<usize> = pagine.into_iter().collect();
        v.sort_unstable();
        v.dedup();
        Toccate::Alcune(v)
    }

    /// Le pagine di tutte e due.
    pub fn unisci(self, altre: Toccate) -> Toccate {
        match (self, altre) {
            (Toccate::Tutte, _) | (_, Toccate::Tutte) => Toccate::Tutte,
            (Toccate::Nessuna, t) | (t, Toccate::Nessuna) => t,
            (Toccate::Alcune(a), Toccate::Alcune(b)) => Toccate::alcune(a.into_iter().chain(b)),
        }
    }

    /// Quante pagine, per la diagnosi: None per tutte.
    pub fn quante(&self) -> Option<usize> {
        match self {
            Toccate::Nessuna => Some(0),
            Toccate::Alcune(v) => Some(v.len()),
            Toccate::Tutte => None,
        }
    }
}

/// Una modifica da dare al motore, con quello che serve sapere prima: il genere per le firme e le
/// pagine che tocca.
#[derive(Debug, Clone, PartialEq)]
pub struct Modifica {
    pub(crate) edit: Edit,
    pub genere: GenereModifica,
    pub toccate: Toccate,
    /// Un paragrafo riscritto (pagina e paragrafo, da 0) di cui si controlla il carattere: se la
    /// riscrittura lo cambia, il motore del thread la annulla e lo dice, finché chi la chiede non
    /// dice di continuare ([`Modifica::anche_se_cambia_carattere`]).
    pub(crate) carattere: Option<(usize, usize)>,
    /// La correzione di una scansione: la foto da ripulire sul thread del motore, subito prima
    /// della modifica (`correzione`).
    pub(crate) pulizia: Option<crate::correzione::Pulizia>,
}

impl Modifica {
    /// Una modifica qualunque del motore.
    pub fn del_motore(edit: Edit) -> Modifica {
        Modifica { genere: GenereModifica::di(&edit), toccate: Toccate::di(&edit), edit, carattere: None, pulizia: None }
    }

    /// Ruota le pagine `pagine` (da 0) di `gradi`, multiplo di 90, in senso orario.
    pub fn ruota(pagine: Vec<usize>, gradi: i64) -> Modifica {
        Modifica::del_motore(Edit::RotatePages { pages: pagine, degrees: gradi })
    }
}

/// Le firme digitali di un documento davanti a una modifica.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Protezione {
    /// Nessuna firma ancora valida: si modifica liberamente.
    #[default]
    Nessuna,
    /// Almeno una firma integra (anche «non verificata», senza certificati fidati); con
    /// `certificazione` il permesso DocMDP della certificazione (1 nessuna modifica, 2 moduli, 3
    /// moduli e commenti), il più stretto se ce n'è più d'una.
    Firmato { certificazione: Option<u8> },
}

impl Protezione {
    /// Dalle firme che il motore ha controllato: contano quelle che ci sono e non sono già non
    /// valide (una firma già rotta non si rompe di più).
    pub fn da(firme: &[SignatureInfo]) -> Protezione {
        let intatte: Vec<&SignatureInfo> = firme.iter().filter(|s| s.signed && s.status != SignatureStatus::Invalid).collect();
        if intatte.is_empty() {
            return Protezione::Nessuna;
        }
        Protezione::Firmato { certificazione: intatte.iter().filter_map(|s| s.certify).min() }
    }

    pub fn firmato(self) -> bool {
        self != Protezione::Nessuna
    }

    /// Una certificazione «nessuna modifica»: il PDF si può solo leggere.
    pub fn sola_lettura(self) -> bool {
        self == Protezione::Firmato { certificazione: Some(1) }
    }

    /// Una modifica di genere `genere` renderebbe non valida una firma.
    pub fn invalida(self, genere: GenereModifica) -> bool {
        let Protezione::Firmato { certificazione } = self else { return false };
        match genere {
            GenereModifica::Metadati => false,
            GenereModifica::Moduli => certificazione == Some(1),
            GenereModifica::Commenti => matches!(certificazione, Some(1 | 2)),
            GenereModifica::Pagine => true,
        }
    }
}

/// Quante firme sono integre e con le modifiche dopo permesse: il motore del thread le conta
/// prima e dopo ogni modifica, per sapere se ne ha rovinata qualcuna.
pub(crate) fn firme_intatte(firme: &[SignatureInfo]) -> usize {
    use pdfcraft_engine::sign::Modification;
    firme.iter().filter(|s| s.signed && s.status != SignatureStatus::Invalid && !matches!(s.modification, Modification::Disallowed(_))).count()
}

#[cfg(test)]
mod test {
    use super::*;
    use pdfcraft_engine::{NewAnnotation, NoteIcon, Shape, Style};

    fn nota(pagina: usize) -> Edit {
        let shape = Shape::Note { at: [100.0, 700.0], icon: NoteIcon::Comment };
        let style = Style::default_for(&shape);
        Edit::AddAnnotation(NewAnnotation { page: pagina, shape, style, contents: String::new(), author: String::new() })
    }

    #[test]
    fn il_genere_delle_modifiche_per_le_firme() {
        assert_eq!(GenereModifica::di(&Edit::RotatePages { pages: vec![0], degrees: 90 }), GenereModifica::Pagine);
        assert_eq!(GenereModifica::di(&nota(0)), GenereModifica::Commenti);
        assert_eq!(GenereModifica::di(&Edit::DeleteAnnotation { page: 0, index: 0 }), GenereModifica::Commenti);
        assert_eq!(GenereModifica::di(&Edit::SetInfo { key: "Title".into(), value: "x".into() }), GenereModifica::Metadati);
        assert_eq!(GenereModifica::di(&Edit::DeletePages { pages: vec![1] }), GenereModifica::Pagine);
        // Un gruppo vale la sua parte meno permessa.
        let gruppo = |edits| Edit::Batch { label: "prova".into(), edits };
        assert_eq!(GenereModifica::di(&gruppo(vec![nota(0), Edit::SetInfo { key: "Title".into(), value: "x".into() }])), GenereModifica::Commenti);
        assert_eq!(GenereModifica::di(&gruppo(vec![nota(0), Edit::RotatePages { pages: vec![0], degrees: 90 }])), GenereModifica::Pagine);
        assert_eq!(GenereModifica::di(&gruppo(vec![])), GenereModifica::Metadati);
        assert_eq!(GenereModifica::Pagine.nome(), "pagine");
    }

    #[test]
    fn le_pagine_toccate() {
        assert_eq!(Toccate::di(&Edit::RotatePages { pages: vec![3, 1, 3], degrees: 90 }), Toccate::Alcune(vec![1, 3]));
        assert_eq!(Toccate::di(&nota(2)), Toccate::Alcune(vec![2]));
        assert_eq!(Toccate::di(&Edit::SetInfo { key: "Title".into(), value: "x".into() }), Toccate::Nessuna);
        assert_eq!(Toccate::di(&Edit::DeletePages { pages: vec![1] }), Toccate::Tutte);
        // Spostare, ingrandire e riscrivere un segno di Fill & Sign toccano la sua pagina.
        assert_eq!(Toccate::di(&Edit::MoveAnnotation { page: 2, index: 0, dx: 1.0, dy: 1.0 }), Toccate::Alcune(vec![2]));
        assert_eq!(Toccate::di(&Edit::FontSizeAnnotation { page: 1, index: 0, size: 12.0 }), Toccate::Alcune(vec![1]));
        let gruppo = Edit::Batch { label: "prova".into(), edits: vec![nota(4), nota(0), Edit::SetInfo { key: "Title".into(), value: "x".into() }] };
        assert_eq!(Toccate::di(&gruppo), Toccate::Alcune(vec![0, 4]));
        assert_eq!(Toccate::Alcune(vec![1]).unisci(Toccate::Tutte), Toccate::Tutte);
        assert_eq!((Toccate::Nessuna.quante(), Toccate::Alcune(vec![1, 2]).quante(), Toccate::Tutte.quante()), (Some(0), Some(2), None));
        let m = Modifica::ruota(vec![0], 90);
        assert_eq!((m.genere, m.toccate), (GenereModifica::Pagine, Toccate::Alcune(vec![0])));
    }

    #[test]
    fn che_cosa_permettono_le_firme() {
        use GenereModifica::*;
        let nessuna = Protezione::Nessuna;
        let approvazione = Protezione::Firmato { certificazione: None };
        let certificata = |n| Protezione::Firmato { certificazione: Some(n) };
        for g in [Metadati, Moduli, Commenti, Pagine] {
            assert!(!nessuna.invalida(g));
        }
        assert_eq!([Metadati, Moduli, Commenti, Pagine].map(|g| approvazione.invalida(g)), [false, false, false, true]);
        assert_eq!([Metadati, Moduli, Commenti, Pagine].map(|g| certificata(3).invalida(g)), [false, false, false, true]);
        assert_eq!([Metadati, Moduli, Commenti, Pagine].map(|g| certificata(2).invalida(g)), [false, false, true, true]);
        assert_eq!([Metadati, Moduli, Commenti, Pagine].map(|g| certificata(1).invalida(g)), [false, true, true, true]);
        assert!(certificata(1).sola_lettura() && !certificata(2).sola_lettura() && !approvazione.sola_lettura());
        assert!(approvazione.firmato() && !nessuna.firmato());
    }
}
