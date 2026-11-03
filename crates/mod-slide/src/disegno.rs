//! Il thread di disegno delle Slide: riceve dal lettore quali slide servono e a che larghezza, le
//! disegna una alla volta con `render_slide` e manda indietro le immagini. Le richieste nuove
//! prendono il posto di quelle che aspettano: conta solo quello che serve adesso. Lo stato del
//! disegno di DeckCraft è per thread (`RENDERER`, ricerca DeckCraft, sezione 3): un thread solo,
//! che tiene il suo.

use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

use deckcraft_anim::{ShowState, Timeline};
use deckcraft_geom::Vec2;
use deckcraft_model::{Presentation, ShapeId};
use deckcraft_render::{ParaState, ShapeState};

use crate::apertura::Immagine;

/// A chi serve una slide disegnata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Uso {
    /// Lo schermo del telefono: il lettore o la presentazione.
    Vista,
    /// La griglia delle miniature.
    Miniatura,
    /// Lo schermo esterno della presentazione: i pixel vanno al proiettore, non in una texture.
    Proiettore,
}

/// La slide `indice` da disegnare larga `larghezza` pixel, per l'uso `uso`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Richiesta {
    pub indice: usize,
    pub larghezza: u32,
    pub uso: Uso,
    /// Nella presentazione, quanti passi delle animazioni sono fatti: le forme che entrano dopo
    /// non ci sono ancora. None nel lettore: la slide com'è nel file, tutta.
    pub passo: Option<usize>,
}

/// Una slide disegnata, con quanto ci è voluto.
pub struct Disegnata {
    pub richiesta: Richiesta,
    pub immagine: Immagine,
    pub ms: u32,
}

/// Le richieste che aspettano, e se il lettore se n'è andato.
#[derive(Default)]
struct Coda {
    richieste: VecDeque<Richiesta>,
    chiusa: bool,
}

/// Il thread di disegno di una presentazione: si ferma quando il `Disegnatore` esce.
pub struct Disegnatore {
    coda: Arc<(Mutex<Coda>, Condvar)>,
    arrivate: Receiver<Disegnata>,
}

impl Disegnatore {
    /// Il thread `slide-disegno` sulla presentazione `doc`; `sveglia` si chiama a ogni slide
    /// disegnata (il lettore chiede un frame).
    pub fn avvia(doc: Arc<Presentation>, sveglia: impl Fn() + Send + 'static) -> std::io::Result<Self> {
        let coda: Arc<(Mutex<Coda>, Condvar)> = Arc::default();
        let (manda, arrivate) = channel();
        let sua = coda.clone();
        std::thread::Builder::new().name("slide-disegno".into()).spawn(move || {
            while let Some(richiesta) = prossima(&sua) {
                let inizio = Instant::now();
                let immagine = disegna(&doc, richiesta);
                let ms = inizio.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
                if manda.send(Disegnata { richiesta, immagine, ms }).is_err() {
                    break;
                }
                sveglia();
            }
        })?;
        Ok(Self { coda, arrivate })
    }

    /// Le slide che servono adesso, nell'ordine in cui servono: prendono il posto di quelle che
    /// aspettavano. Quella che si sta disegnando finisce comunque.
    pub fn chiedi(&self, richieste: impl IntoIterator<Item = Richiesta>) {
        let (coda, segnale) = &*self.coda;
        let mut coda = coda.lock().unwrap_or_else(|e| e.into_inner());
        coda.richieste = richieste.into_iter().collect();
        segnale.notify_one();
    }

    /// Una slide disegnata, se ce n'è una pronta.
    pub fn prendi(&self) -> Option<Disegnata> {
        self.arrivate.try_recv().ok()
    }

    /// Ci sono richieste che aspettano il thread.
    pub fn in_coda(&self) -> bool {
        let (coda, _) = &*self.coda;
        !coda.lock().unwrap_or_else(|e| e.into_inner()).richieste.is_empty()
    }
}

impl Drop for Disegnatore {
    /// Chiude la coda e lascia andare il thread: la slide che sta disegnando la finisce da solo,
    /// senza mandarla, e si ferma. Aspettarlo fermava l'interfaccia di chi chiude: una slide con
    /// le foto larga 4096 pixel è 80 ms sul PC, centinaia sul telefono.
    fn drop(&mut self) {
        let (coda, segnale) = &*self.coda;
        coda.lock().unwrap_or_else(|e| e.into_inner()).chiusa = true;
        segnale.notify_one();
    }
}

/// La prossima richiesta, aspettando che ce ne sia una; None quando il lettore se n'è andato.
fn prossima(coda: &(Mutex<Coda>, Condvar)) -> Option<Richiesta> {
    let (coda, segnale) = coda;
    let mut c = coda.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if c.chiusa {
            return None;
        }
        if let Some(r) = c.richieste.pop_front() {
            return Some(r);
        }
        c = segnale.wait(c).unwrap_or_else(|e| e.into_inner());
    }
}

/// La slide della richiesta, larga proprio `larghezza` pixel e alta in proporzione.
fn disegna(doc: &Presentation, r: Richiesta) -> Immagine {
    let (w, h) = (doc.slide_size.width, doc.slide_size.height);
    let larghezza = r.larghezza.clamp(1, deckcraft_render::MAX_SIDE);
    let scala = if w > 0.0 { f64::from(larghezza) / w } else { 1.0 };
    let altezza = ((h * scala).round() as u32).clamp(1, deckcraft_render::MAX_SIDE);
    let mut opzioni = deckcraft_render::RenderOpts { scale: scala, size: Some((larghezza, altezza)), ..Default::default() };
    let img = match r.passo {
        Some(passo) => {
            let tl = ShowState::timeline(doc, r.indice);
            let stato = |id: ShapeId| stato_della_forma(&tl, id, passo);
            opzioni.state = Some(&stato);
            deckcraft_render::render_slide(doc, r.indice, &opzioni)
        }
        None => deckcraft_render::render_slide(doc, r.indice, &opzioni),
    };
    Immagine { larghezza: img.width, altezza: img.height, pixel: img.pixels }
}

/// La larghezza in pixel a cui disegnare una slide di `w` × `h` punti che si vorrebbe larga
/// `voluta` pixel, col lato più lungo al più `lato_massimo` pixel: l'altezza la arrotonda
/// `disegna`, e anche lei deve restare sotto il lato.
pub(crate) fn larghezza_entro(w: f64, h: f64, voluta: u32, lato_massimo: u32) -> u32 {
    let (w, h) = (w.max(1.0), h.max(1.0));
    let altezza = |l: u32| (h * f64::from(l) / w).round();
    let mut l = voluta.clamp(1, lato_massimo.max(1));
    if altezza(l) > f64::from(lato_massimo) {
        l = ((f64::from(lato_massimo) * w / h).floor() as u32).max(1);
        while l > 1 && altezza(l) > f64::from(lato_massimo) {
            l -= 1;
        }
    }
    l
}

/// Lo stato della forma `id` quando sono fatti `passo` passi della presentazione, e il passo dopo
/// non è cominciato: la forma e, nelle entrate per paragrafo, ognuno dei suoi paragrafi
/// (`state_for` della presentazione di DeckCraft, `crates/ui-egui/src/show.rs`, senza le
/// sequenze a comando).
fn stato_della_forma(tl: &Timeline, id: ShapeId, passo: usize) -> ShapeState {
    let a = tl.state(id, None, passo, 0.0);
    ShapeState {
        visible: a.visible,
        opacity: a.opacity,
        offset: Vec2::new(a.offset_x, a.offset_y),
        scale: (a.scale_x, a.scale_y),
        rotate: a.rotate,
        clip: a.clip,
        tint: a.tint,
        paras: tl
            .paragraph_targets(id)
            .into_iter()
            .map(|p| {
                let a = tl.state(id, Some(p), passo, 0.0);
                (p, ParaState { visible: a.visible, opacity: a.opacity, offset: Vec2::new(a.offset_x, a.offset_y) })
            })
            .collect(),
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::apertura::Presentazione;
    use std::time::Duration;

    fn disegnatore() -> Disegnatore {
        let p = Presentazione::apri(crate::prova::LEZIONE).unwrap();
        Disegnatore::avvia(p.documento().clone(), || {}).unwrap()
    }

    /// Le slide disegnate fino a quella chiesta `fino`, al massimo 10 s.
    fn fino_a(d: &Disegnatore, fino: Richiesta) -> Vec<Disegnata> {
        let fine = Instant::now() + Duration::from_secs(10);
        let mut arrivate = Vec::new();
        loop {
            assert!(Instant::now() < fine, "la slide {fino:?} non è arrivata in 10 s");
            match d.prendi() {
                Some(a) => {
                    let finito = a.richiesta == fino;
                    arrivate.push(a);
                    if finito {
                        return arrivate;
                    }
                }
                None => std::thread::sleep(Duration::from_millis(2)),
            }
        }
    }

    #[test]
    fn una_slide_chiesta_torna_della_misura_giusta() {
        let d = disegnatore();
        let r = Richiesta { indice: 2, larghezza: 481, uso: Uso::Vista, passo: None };
        d.chiedi([r]);
        let a = fino_a(&d, r).pop().unwrap();
        // 481 × 540 / 960 = 270,56: l'altezza si arrotonda.
        assert_eq!((a.immagine.larghezza, a.immagine.altezza), (481, 271));
        assert_eq!(a.immagine.pixel.len(), 481 * 271 * 4);
        assert!(!d.in_coda());
    }

    #[test]
    fn le_richieste_nuove_prendono_il_posto_di_quelle_che_aspettano() {
        let d = disegnatore();
        d.chiedi((0..4).map(|indice| Richiesta { indice, larghezza: 1600, uso: Uso::Vista, passo: None }));
        let voluta = Richiesta { indice: 3, larghezza: 100, uso: Uso::Miniatura, passo: None };
        d.chiedi([voluta]);
        // Al più quella che il thread aveva già preso prima della seconda richiesta.
        let arrivate = fino_a(&d, voluta);
        assert!(arrivate.len() <= 2, "{} slide disegnate", arrivate.len());
    }

    /// I pixel scuri della fascia dei punti della slide 2 (da 160 a 280 punti), disegnata larga
    /// 960 pixel coi passi fatti `passo`.
    fn scuri_nei_punti(d: &Disegnatore, passo: usize) -> usize {
        let r = Richiesta { indice: 1, larghezza: 960, uso: Uso::Vista, passo: Some(passo) };
        d.chiedi([r]);
        let img = fino_a(d, r).pop().unwrap().immagine;
        img.pixel[160 * 960 * 4..280 * 960 * 4].as_chunks::<4>().0.iter().filter(|p| p[0] < 100 && p[3] == 255).count()
    }

    #[test]
    fn nella_presentazione_i_punti_compaiono_coi_passi() {
        let d = disegnatore();
        let (zero, uno, tre) = (scuri_nei_punti(&d, 0), scuri_nei_punti(&d, 1), scuri_nei_punti(&d, 3));
        // Prima del primo passo i tre punti non ci sono; poi uno, poi tutti e tre.
        assert!(zero < 20, "{zero} pixel scuri prima dei passi");
        assert!(uno > 100 && tre > 2 * uno, "un passo {uno}, tre passi {tre}");
    }

    #[test]
    fn la_larghezza_resta_sotto_il_lato_massimo_anche_per_l_altezza() {
        // Una slide 16:9 larga 2494 pixel con texture fino a 2048: larga 2048.
        assert_eq!(larghezza_entro(960.0, 540.0, 2494, 2048), 2048);
        // Sotto il lato resta com'è.
        assert_eq!(larghezza_entro(960.0, 540.0, 998, 2048), 998);
        // Una slide in verticale: è l'altezza a toccare il lato.
        let l = larghezza_entro(540.0, 960.0, 2000, 2048);
        assert_eq!(l, 1152);
        assert!((960.0 * f64::from(l) / 540.0).round() <= 2048.0);
    }

    #[test]
    fn senza_lettore_il_thread_si_ferma() {
        let d = disegnatore();
        d.chiedi([Richiesta { indice: 0, larghezza: 800, uso: Uso::Vista, passo: None }]);
        let inizio = Instant::now();
        drop(d);
        assert!(inizio.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn chiudere_non_aspetta_la_slide_che_si_disegna_e_il_thread_finisce_da_solo() {
        // La slide con la foto, larga 4096 pixel: sul PC 80 ms, sul telefono centinaia, che
        // chiudendo erano un'interfaccia ferma.
        let p = Presentazione::apri(crate::prova::LEZIONE).unwrap();
        let doc = p.documento().clone();
        let d = Disegnatore::avvia(doc.clone(), || {}).unwrap();
        d.chiedi([Richiesta { indice: 2, larghezza: 4096, uso: Uso::Vista, passo: None }]);
        std::thread::sleep(Duration::from_millis(3));
        let inizio = Instant::now();
        drop(d);
        assert!(inizio.elapsed() < Duration::from_millis(20), "la chiusura ha aspettato {:?}", inizio.elapsed());
        // Il thread lascia la presentazione appena ha finito.
        let fine = Instant::now() + Duration::from_secs(20);
        while Arc::strong_count(&doc) > 2 {
            assert!(Instant::now() < fine, "il thread non è finito");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
