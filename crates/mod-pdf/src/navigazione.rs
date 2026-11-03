//! Orientarsi in un PDF: l'indice dai segnalibri (`/Outlines`), i link delle pagine e il testo di
//! una pagina. Tutto dalle informazioni che il motore legge all'apertura (`DocInfo::outline` e
//! `DocInfo::links`), con le posizioni portate nella pagina come si vede: pt dall'alto a sinistra,
//! dopo `/Rotate`, come il testo e il lettore. Titoli e indirizzi sono testo del documento: mai
//! nella diagnosi.

use std::sync::Arc;

use pdfcraft_render::{DestView, LinkTarget, PageInfo, PageRenderer, PageText, RenderRequest, RequestKind};

use crate::apertura::Pdf;
use crate::ricerca::RettangoloPt;

/// Una voce dell'indice, nell'ordine dei segnalibri, con il suo livello (0 è il primo).
#[derive(Debug, Clone, PartialEq)]
pub struct Segnalibro {
    pub titolo: String,
    pub livello: usize,
    /// La pagina (da 0); None per un segnalibro che non porta a una pagina.
    pub pagina: Option<usize>,
    /// Dove nella pagina, in pt dall'alto, se il segnalibro lo dice.
    pub y: Option<f32>,
}

/// Dove porta un link.
#[derive(Debug, Clone, PartialEq)]
pub enum Destinazione {
    /// Una pagina di questo PDF (da 0) e, se c'è, il punto in pt dall'alto.
    Pagina { pagina: usize, y: Option<f32> },
    /// Un indirizzo fuori dal PDF (una pagina web, una mail).
    Indirizzo(String),
    /// Un altro file, un livello da accendere, un'azione: niente che il lettore faccia.
    Altro,
}

/// Un link in una pagina: il suo rettangolo in pt nella pagina come si vede.
#[derive(Debug, Clone, PartialEq)]
pub struct Collegamento {
    pub pagina: usize,
    /// La sua posizione in `/Annots` della pagina (come lo indicano le modifiche).
    pub indice: usize,
    pub rettangolo: RettangoloPt,
    pub destinazione: Destinazione,
}

/// Un titolo da mostrare su una riga: senza a capo né caratteri di controllo, senza spazi ai lati.
fn titolo_pulito(titolo: &str) -> String {
    let una_riga: String = titolo.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let pulito = una_riga.split_whitespace().collect::<Vec<_>>().join(" ");
    if pulito.is_empty() { "Senza titolo".to_string() } else { pulito }
}

/// Il punto in pt dall'alto della pagina a cui porta una destinazione, se lo dice.
fn y_della_destinazione(pagina: &PageInfo, vista: DestView) -> Option<f32> {
    let (x, y) = match vista {
        DestView::Xyz { left, top, .. } => (left, top),
        DestView::FitH { top } => (None, top),
        DestView::FitV { left } => (left, None),
        DestView::FitR { rect } => (Some(rect[0]), Some(rect[3])),
        DestView::Top | DestView::Fit => (None, None),
    };
    let [_, frazione] = pagina.dest_fraction(x, y, 0);
    frazione.map(|f| f * pagina.height)
}

impl Pdf {
    /// I segnalibri in ordine, coi livelli; e se il PDF ne ha altri che il motore non ha letto
    /// (oltre 10 000, o più di 32 livelli).
    pub fn segnalibri(&self) -> (Vec<Segnalibro>, bool) {
        let info = &self.documento().info;
        let mut fuori = Vec::new();
        let mut pila: Vec<(usize, &pdfcraft_render::OutlineItem)> = info.outline.iter().rev().map(|v| (0, v)).collect();
        while let Some((livello, voce)) = pila.pop() {
            let pagina = voce.page.filter(|p| *p < info.pages.len());
            let y = pagina.and_then(|p| y_della_destinazione(&info.pages[p], voce.view));
            fuori.push(Segnalibro { titolo: titolo_pulito(&voce.title), livello, pagina, y });
            pila.extend(voce.children.iter().rev().map(|f| (livello + 1, f)));
        }
        (fuori, info.outline_more)
    }

    /// I link di tutte le pagine, coi rettangoli nella pagina come si vede.
    pub fn collegamenti(&self) -> Vec<Collegamento> {
        let info = &self.documento().info;
        info.links
            .iter()
            .filter_map(|l| {
                let pagina = info.pages.get(l.page)?;
                let [x0, y0, x1, y1] = l.rect;
                let (a, b) = (pagina.user_to_view(x0, y0), pagina.user_to_view(x1, y1));
                let rettangolo = [a[0].min(b[0]), a[1].min(b[1]), a[0].max(b[0]), a[1].max(b[1])];
                if !rettangolo.iter().all(|v| v.is_finite()) {
                    return None;
                }
                let destinazione = match &l.target {
                    LinkTarget::Page(p, vista) if *p < info.pages.len() => {
                        Destinazione::Pagina { pagina: *p, y: y_della_destinazione(&info.pages[*p], *vista) }
                    }
                    LinkTarget::Uri(indirizzo) if !indirizzo.trim().is_empty() => Destinazione::Indirizzo(indirizzo.trim().to_string()),
                    _ => Destinazione::Altro,
                };
                Some(Collegamento { pagina: l.page, indice: l.index, rettangolo, destinazione })
            })
            .collect()
    }

    /// Il testo della pagina `pagina` (da 0), letto sul thread che chiama: per le prove e per chi
    /// non ha un pool. Il lettore lo chiede al suo pool.
    pub fn testo(&self, pagina: usize) -> Option<Arc<PageText>> {
        let configurazione = self.sessione.render_config(self.id).unwrap_or_default();
        let mut motore = PageRenderer::new(self.documento().display.clone(), configurazione);
        if pagina >= motore.page_count() {
            return None;
        }
        motore.render(RenderRequest { page: pagina, kind: RequestKind::Text, ..Default::default() }).text
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prova::{Bersaglio, Collegamento as Link, Pagina, Segnalibro as Voce, pdf, pdf_completo};

    fn a4() -> Pagina<'static> {
        Pagina::bianca(595.0, 842.0)
    }

    #[test]
    fn i_segnalibri_in_ordine_coi_livelli_e_le_pagine() {
        let voci = [
            Voce {
                titolo: "Perché leggere",
                pagina: 0,
                y: None,
                figli: vec![Voce { titolo: "Dati\ndel paziente", pagina: 1, y: Some(642.0), figli: vec![] }],
            },
            Voce { titolo: "  ", pagina: 2, y: None, figli: vec![] },
        ];
        let p = Pdf::apri(pdf_completo(&[a4(), a4(), a4()], &voci, &[]), None).unwrap();
        let (indice, altri) = p.segnalibri();
        assert!(!altri);
        let titoli: Vec<_> = indice.iter().map(|s| (s.titolo.as_str(), s.livello, s.pagina)).collect();
        assert_eq!(titoli, [("Perché leggere", 0, Some(0)), ("Dati del paziente", 1, Some(1)), ("Senza titolo", 0, Some(2))]);
        // 642 pt dal fondo di una pagina alta 842: 200 dall'alto.
        assert!((indice[1].y.unwrap() - 200.0).abs() < 0.5, "{:?}", indice[1].y);
        assert_eq!(indice[0].y, None);
        // Un PDF senza segnalibri.
        let senza = Pdf::apri(pdf(&[a4()]), None).unwrap();
        assert_eq!(senza.segnalibri(), (Vec::new(), false));
    }

    #[test]
    fn i_link_coi_rettangoli_nella_pagina_e_le_destinazioni() {
        let link = [
            Link { pagina: 0, rettangolo: [50.0, 700.0, 250.0, 720.0], bersaglio: Bersaglio::Pagina(2, Some(442.0)) },
            Link { pagina: 1, rettangolo: [100.0, 100.0, 200.0, 120.0], bersaglio: Bersaglio::Indirizzo("https://esempio.it/pagina") },
        ];
        let p = Pdf::apri(pdf_completo(&[a4(), a4(), a4()], &[], &link), None).unwrap();
        let tutti = p.collegamenti();
        assert_eq!(tutti.len(), 2);
        // Nella pagina come si vede: y dall'alto.
        assert_eq!(tutti[0].pagina, 0);
        assert_eq!(tutti[0].rettangolo, [50.0, 122.0, 250.0, 142.0]);
        match tutti[0].destinazione {
            Destinazione::Pagina { pagina: 2, y: Some(y) } => assert!((y - 400.0).abs() < 0.5, "{y}"),
            ref d => panic!("{d:?}"),
        }
        assert_eq!(tutti[1].destinazione, Destinazione::Indirizzo("https://esempio.it/pagina".into()));
        assert_eq!(tutti[1].rettangolo, [100.0, 722.0, 200.0, 742.0]);
    }

    #[test]
    fn il_testo_di_una_pagina() {
        let pagina = Pagina { larghezza: 595.0, altezza: 842.0, rotazione: 0, contenuto: "BT /F1 12 Tf 50 700 Td (Ciao mondo) Tj ET" };
        let p = Pdf::apri(pdf(&[pagina, a4()]), None).unwrap();
        assert_eq!(p.testo(0).unwrap().plain_text(), "Ciao mondo");
        assert!(p.testo(1).unwrap().is_empty());
        assert!(p.testo(2).is_none());
    }
}
