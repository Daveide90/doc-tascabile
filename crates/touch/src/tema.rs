//! Il tema «carta e inchiostro» dei bozzetti (design/bozzetti/stile.css): colori, misure, raggi,
//! ombre, caratteri e lo stile chiaro di egui. Le misure sono quelle del CSS: 1 px = 1 dp = 1 punto
//! di egui.

use std::sync::{Arc, LazyLock};

use egui::{
    Color32, Context, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Painter, Rect, Shadow, Shape, Stroke, Style, TextStyle,
    Theme, ThemePreference, Vec2, Visuals, vec2,
};

// Colori di `:root` in stile.css.

/// Il fondo dell'app.
pub const CARTA: Color32 = Color32::from_rgb(0xF6, 0xF3, 0xEE);
/// Superfici e pagina.
pub const FOGLIO: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
/// Il testo principale.
pub const INCHIOSTRO: Color32 = Color32::from_rgb(0x1E, 0x1B, 0x18);
/// Il testo secondario.
pub const MATITA: Color32 = Color32::from_rgb(0x6A, 0x63, 0x5B);
/// Le linee sottili.
pub const FILO: Color32 = Color32::from_rgb(0xE4, 0xDE, 0xD4);
/// Le superfici leggere: gruppi, passi, pulsanti tenui.
pub const VELO: Color32 = Color32::from_rgb(0xED, 0xE8, 0xE0);
/// Word e il colore dell'app.
pub const BLU: Color32 = Color32::from_rgb(0x23, 0x46, 0xA0);
pub const BLU_TENUE: Color32 = Color32::from_rgb(0xE3, 0xE9, 0xF7);
/// PDF.
pub const ROSSO: Color32 = Color32::from_rgb(0xC2, 0x41, 0x2B);
pub const ROSSO_TENUE: Color32 = Color32::from_rgb(0xF7, 0xE4, 0xDF);
/// Slide.
pub const OCRA: Color32 = Color32::from_rgb(0xB7, 0x79, 0x1A);
pub const OCRA_TENUE: Color32 = Color32::from_rgb(0xF6, 0xEB, 0xD6);
/// Le barre scure: la barra della selezione, gli avvisi brevi.
pub const SCURO: Color32 = Color32::from_rgb(0x2A, 0x26, 0x22);

// Colori di pdf.css.

/// Il tavolo sotto le pagine di un PDF (`.tavolo`): un poco più scuro della carta, perché le
/// pagine bianche si stacchino.
pub const TAVOLO: Color32 = Color32::from_rgb(0xE9, 0xE5, 0xDE);
/// L'indicatore della pagina sopra il PDF (`.numero-pag`: inchiostro al 78%).
pub const INDICATORE: Color32 = Color32::from_rgba_unmultiplied_const(30, 27, 24, 199);
/// I risultati della ricerca sulla pagina: giallo tenue, che lascia leggere il testo sotto.
pub const TROVATO: Color32 = Color32::from_rgba_unmultiplied_const(255, 214, 0, 80);
/// Il risultato attuale della ricerca: più forte, verso l'arancio.
pub const TROVATO_ATTUALE: Color32 = Color32::from_rgba_unmultiplied_const(255, 166, 0, 150);

// Colori che stanno nelle regole di stile.css, e quelli per il dito che preme (il CSS non li ha).

/// L'icona di un pulsante spento (`.ib.spento`).
pub const SPENTO: Color32 = Color32::from_rgb(0xB9, 0xB2, 0xA8);
/// La spunta di «Salvato» (`.barra-alta .titolo .s .i`).
pub const VERDE: Color32 = Color32::from_rgb(0x3C, 0x8A, 0x55);
/// Il fondo del riquadro dei fogli di calcolo.
pub const VERDE_TENUE: Color32 = Color32::from_rgb(0xE2, 0xF0, 0xE6);
/// La lineetta in cima al foglio dal fondo (`.maniglia`).
pub const LINEETTA: Color32 = Color32::from_rgb(0xCF, 0xC8, 0xBD);
/// Il velo sullo schermo sotto un foglio o un dialogo (`.velo`: inchiostro al 38%).
pub const VELO_SCURO: Color32 = Color32::from_rgba_unmultiplied_const(30, 27, 24, 97);
/// Il fondo del testo selezionato (`.sel`: blu al 18%).
pub const SELEZIONE: Color32 = Color32::from_rgba_unmultiplied_const(35, 70, 160, 46);
/// Le linee sulla barra scura (`.contesto .sep`: bianco al 18%).
pub const FILO_SCURO: Color32 = Color32::from_rgba_unmultiplied_const(255, 255, 255, 46);
/// Sotto il dito, su carta e foglio.
pub const PREMUTO: Color32 = VELO;
/// Sotto il dito, su velo e su blu tenue.
pub const PREMUTO_VELO: Color32 = Color32::from_rgb(0xDD, 0xD6, 0xCB);
/// Sotto il dito, sul blu.
pub const PREMUTO_BLU: Color32 = Color32::from_rgb(0x1A, 0x36, 0x80);
/// Sotto il dito, sullo scuro.
pub const PREMUTO_SCURO: Color32 = Color32::from_rgb(0x45, 0x3F, 0x39);

// Misure, in dp.

/// Il lato di un pulsante icona (`.ib`), che è anche il bersaglio più piccolo per il dito.
pub const PULSANTE: f32 = 48.0;
/// Le icone (`.i`).
pub const ICONA: f32 = 24.0;
/// La barra alta (`.barra-alta`).
pub const BARRA_ALTA: f32 = 64.0;
/// Una voce di elenco (`.voce`).
pub const VOCE: f32 = 72.0;
/// Il riquadro del tipo in una voce (`.tipo`).
pub const TIPO: f32 = 44.0;
/// Il chip (`.chip`).
pub const CHIP: f32 = 34.0;
/// Il pulsante «Nuovo» flottante (`.fab`).
pub const NUOVO: f32 = 58.0;
/// La barra di formato flottante (`.formato`).
pub const FORMATO: f32 = 60.0;
/// La barra di formato appoggiata sulla tastiera (`.formato` nel bozzetto 3).
pub const FORMATO_SULLA_TASTIERA: f32 = 52.0;
/// La barra del contesto (`.contesto`).
pub const CONTESTO: f32 = 52.0;
/// La barra flottante con le voci a icona e nome del lettore PDF (`.modi`).
pub const VOCI: f32 = 64.0;
/// La larghezza del menu (`.menu`).
pub const MENU: f32 = 252.0;
/// Una voce del menu (`.menu .m`).
pub const VOCE_MENU: f32 = 48.0;
/// Un segmento di un gruppo o di un passo (`.gruppo .ib`).
pub const SEGMENTO: Vec2 = vec2(46.0, 44.0);
/// Il pulsante largo con icona ed etichetta (`.bott`).
pub const PULSANTE_LARGO: f32 = 52.0;

// Spazi, in dp.

/// Dal bordo dello schermo alle schede (`.elenco`) e alla barra flottante (`.formato`).
pub const MARGINE: f32 = 14.0;
/// Dal bordo dello schermo ai contenuti dei fogli e alla ricerca (`.sotto`, `.cerca`, `.filtri`).
pub const MARGINE_LARGO: f32 = 18.0;
/// Dal bordo dello schermo ai titoli (`.titoletto`, `.home-testa`).
pub const MARGINE_TITOLO: f32 = 22.0;
/// Fra due pulsanti di una fila (`.filtri`, `.stili`).
pub const SPAZIO: f32 = 8.0;
/// Le barre flottanti dal fondo della zona libera: nel bozzetto `.formato` sta a 34 dp dal fondo
/// dello schermo, cioè 12 sopra i 22 della barra di navigazione, che nell'app è fuori dalla zona.
pub const SOPRA_IL_FONDO: f32 = 12.0;
/// Con la tastiera aperta la barra di formato sta a 8 dp dalla tastiera e dai lati (bozzetto 3).
pub const SOPRA_LA_TASTIERA: f32 = 8.0;

// Raggi, in dp.

/// La scheda dell'elenco (`.elenco`).
pub const RAGGIO_SCHEDA: u8 = 20;
/// Le cartelle (`.cartella`).
pub const RAGGIO_CARTELLA: u8 = 16;
/// Il foglio dal fondo (`.sotto`).
pub const RAGGIO_FOGLIO: u8 = 28;
/// Menu e dialoghi (`.menu`).
pub const RAGGIO_MENU: u8 = 18;
/// Gruppi, passi e pulsanti larghi (`.gruppo`, `.passo`, `.bott`).
pub const RAGGIO_GRUPPO: u8 = 16;
/// I segmenti dei gruppi e il riquadro del tipo (`.gruppo .ib`, `.tipo`).
pub const RAGGIO_SEGMENTO: u8 = 12;
/// Il pulsante «Nuovo» (`.fab`).
pub const RAGGIO_NUOVO: u8 = 18;
/// La pagina dell'editor sulla carta, in alto (`.foglio`).
pub const RAGGIO_PAGINA: u8 = 18;
/// La barra di formato appoggiata sulla tastiera (bozzetto 3).
pub const RAGGIO_FORMATO_SULLA_TASTIERA: u8 = 24;
/// La barra con le voci del lettore PDF (`.modi`), e una sua voce sotto il dito (`.modo`).
pub const RAGGIO_VOCI: u8 = 22;
pub const RAGGIO_VOCE: u8 = 16;

// Ombre.

/// Un'ombra come il `box-shadow` del CSS, con due strati; uno strato trasparente non si disegna.
pub type Ombra = [Shadow; 2];

/// Uno strato `0 dy sfumatura colore` del CSS. Il CSS sfuma con una gaussiana, egui in linea su
/// tutta la larghezza `blur`: una volta e mezza la sfumatura del CSS le somiglia.
const fn strato(dy: i8, sfumatura: u8, colore: Color32) -> Shadow {
    Shadow { offset: [0, dy], blur: (sfumatura as u16 * 3 / 2) as u8, spread: 0, color: colore }
}

const fn inchiostro(alfa: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const(30, 27, 24, alfa)
}

const fn nero(alfa: u8) -> Color32 {
    Color32::from_rgba_unmultiplied_const(0, 0, 0, alfa)
}

/// Schede, ricerca, cartelle e la pagina (`--ombra`).
pub const OMBRA: Ombra = [strato(1, 2, inchiostro(20)), strato(6, 18, inchiostro(20))];
/// La barra di formato flottante (`.formato`).
pub const OMBRA_FLOTTANTE: Ombra = [strato(2, 6, inchiostro(31)), strato(12, 28, inchiostro(36))];
/// Il pulsante «Nuovo» (`.fab`), in blu.
pub const OMBRA_NUOVO: Ombra =
    [strato(4, 10, Color32::from_rgba_unmultiplied_const(35, 70, 160, 89)), strato(10, 24, Color32::from_rgba_unmultiplied_const(35, 70, 160, 64))];
/// Menu e dialoghi (`.menu`).
pub const OMBRA_MENU: Ombra = [strato(8, 30, nero(46)), strato(1, 3, nero(20))];
/// La barra del contesto e gli avvisi brevi (`.contesto`).
pub const OMBRA_CONTESTO: Ombra = [strato(6, 20, nero(64)), Shadow::NONE];
/// Il foglio dal fondo (`.sotto`), verso l'alto.
pub const OMBRA_FOGLIO: Ombra = [strato(-8, 30, nero(46)), Shadow::NONE];
/// Una pagina di un PDF sul tavolo (`.pagina` di pdf.css).
pub const OMBRA_PAGINA: Ombra = [strato(1, 3, nero(31)), strato(6, 16, nero(20))];
/// Il segmento acceso di un gruppo (`.gruppo .ib.acceso`).
pub const OMBRA_ACCESO: Ombra = [strato(1, 3, nero(31)), Shadow::NONE];

/// Le forme dell'ombra di un rettangolo arrotondato, da mettere sotto il rettangolo.
pub fn forme_ombra(rect: Rect, raggio: impl Into<CornerRadius>, ombra: &Ombra) -> Vec<Shape> {
    let raggio = raggio.into();
    ombra.iter().filter(|s| s.color.a() > 0).map(|s| Shape::Rect(s.as_shape(rect, raggio))).collect()
}

/// Disegna l'ombra di un rettangolo arrotondato.
pub fn dipingi_ombra(painter: &Painter, rect: Rect, raggio: impl Into<CornerRadius>, ombra: &Ombra) {
    painter.extend(forme_ombra(rect, raggio, ombra));
}

// Caratteri.

/// Atkinson Hyperlegible Next normale (OFL, fonts/OFL-AtkinsonHyperlegibleNext.txt).
const ATKINSON: &[u8] = include_bytes!("../fonts/AtkinsonHyperlegibleNext-Regular.ttf");
/// Atkinson Hyperlegible Next grassetto.
const ATKINSON_GRASSETTO: &[u8] = include_bytes!("../fonts/AtkinsonHyperlegibleNext-Bold.ttf");
/// Fraunces SemiBold (OFL, fonts/OFL-Fraunces.txt).
const FRAUNCES: &[u8] = include_bytes!("../fonts/Fraunces-SemiBold.ttf");
/// Material Symbols Rounded, il sottoinsieme di design/icone-scelte.txt (Apache 2.0).
const ICONE_VUOTE: &[u8] = include_bytes!("../fonts/MaterialSymbolsRounded-Vuote.ttf");
const ICONE_PIENE: &[u8] = include_bytes!("../fonts/MaterialSymbolsRounded-Piene.ttf");

static GRASSETTO: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("atkinson-grassetto".into()));
static TITOLO: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("fraunces".into()));
static ICONA_VUOTA: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("icone".into()));
static ICONA_PIENA: LazyLock<FontFamily> = LazyLock::new(|| FontFamily::Name("icone-piene".into()));

/// I caratteri del tema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carattere {
    /// Atkinson Hyperlegible Next normale: il proporzionale di egui.
    Testo,
    /// Atkinson Hyperlegible Next grassetto: nomi, chip, pulsanti.
    Grassetto,
    /// Fraunces SemiBold: i titoli.
    Titolo,
    /// Le icone vuote.
    Icona,
    /// Le icone piene (i riquadri del tipo, la spunta di «Salvato»).
    IconaPiena,
}

impl Carattere {
    /// La famiglia di egui di questo carattere.
    pub fn famiglia(self) -> FontFamily {
        match self {
            Carattere::Testo => FontFamily::Proportional,
            Carattere::Grassetto => GRASSETTO.clone(),
            Carattere::Titolo => TITOLO.clone(),
            Carattere::Icona => ICONA_VUOTA.clone(),
            Carattere::IconaPiena => ICONA_PIENA.clone(),
        }
    }

    /// Il carattere è attivo in questo contesto: il tema è installato e il passo in cui lo si è
    /// installato è finito (egui cambia i caratteri al passo dopo).
    pub fn pronto(self, ctx: &Context) -> bool {
        let famiglia = self.famiglia();
        matches!(famiglia, FontFamily::Proportional) || ctx.fonts(|f| f.definitions().families.contains_key(&famiglia))
    }

    /// Il font di questo carattere, in questa misura. Se il carattere non è ancora attivo ripiega
    /// sul proporzionale: egui va in panico con una famiglia che non conosce.
    pub fn font(self, ctx: &Context, misura: f32) -> FontId {
        if self.pronto(ctx) { FontId::new(misura, self.famiglia()) } else { FontId::proportional(misura) }
    }
}

/// I caratteri di egui col tema: il proporzionale è Atkinson normale, con dietro il ripiego che
/// egui ha già (Ubuntu per qualche simbolo, le emoji e le icone di egui come ☰, ↩ e ❌, che l'app
/// usa); in più le famiglie con nome del grassetto, dei titoli e delle icone.
pub fn caratteri() -> FontDefinitions {
    caratteri_con(&[])
}

/// Come [`caratteri`], con in più i caratteri `altri` (nome e byte), ognuno come famiglia con
/// nome col ripiego del proporzionale: quelli delle celle dei fogli di calcolo.
pub fn caratteri_con(altri: &[(&'static str, &'static [u8])]) -> FontDefinitions {
    let mut d = FontDefinitions::default();
    for (nome, byte) in [
        ("atkinson", ATKINSON),
        ("atkinson-grassetto", ATKINSON_GRASSETTO),
        ("fraunces", FRAUNCES),
        ("icone-vuote", ICONE_VUOTE),
        ("icone-piene", ICONE_PIENE),
    ] {
        d.font_data.insert(nome.to_owned(), Arc::new(FontData::from_static(byte)));
    }
    let ripiego = d.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let con_ripiego = |primi: &[&str]| primi.iter().map(|n| n.to_string()).chain(ripiego.iter().cloned()).collect::<Vec<_>>();
    d.families.insert(FontFamily::Proportional, con_ripiego(&["atkinson"]));
    d.families.insert(Carattere::Grassetto.famiglia(), con_ripiego(&["atkinson-grassetto"]));
    d.families.insert(Carattere::Titolo.famiglia(), con_ripiego(&["fraunces", "atkinson"]));
    d.families.insert(Carattere::Icona.famiglia(), vec!["icone-vuote".to_owned()]);
    d.families.insert(Carattere::IconaPiena.famiglia(), vec!["icone-piene".to_owned()]);
    for (nome, byte) in altri {
        d.font_data.insert((*nome).to_owned(), Arc::new(FontData::from_static(byte)));
        d.families.insert(FontFamily::Name((*nome).into()), con_ripiego(&[nome]));
    }
    d
}

// Lo stile di egui.

/// I colori di egui col tema: chiari, carta e inchiostro.
pub fn visuals() -> Visuals {
    let mut v = Visuals::light();
    v.panel_fill = CARTA;
    v.window_fill = FOGLIO;
    v.window_stroke = Stroke::NONE;
    v.window_corner_radius = CornerRadius::same(RAGGIO_MENU);
    v.menu_corner_radius = CornerRadius::same(RAGGIO_MENU);
    v.window_shadow = OMBRA_MENU[0];
    v.popup_shadow = OMBRA_MENU[0];
    v.extreme_bg_color = FOGLIO;
    v.text_edit_bg_color = Some(FOGLIO);
    v.faint_bg_color = VELO;
    v.code_bg_color = VELO;
    v.hyperlink_color = BLU;
    v.warn_fg_color = OCRA;
    v.error_fg_color = ROSSO;
    v.weak_text_color = Some(MATITA);
    v.selection.bg_fill = SELEZIONE;
    v.selection.stroke = Stroke::new(1.0, BLU);
    v.text_cursor.stroke = Stroke::new(2.0, BLU);
    let tondo = CornerRadius::same(RAGGIO_GRUPPO);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = FOGLIO;
    w.noninteractive.weak_bg_fill = CARTA;
    w.noninteractive.bg_stroke = Stroke::new(1.0, FILO);
    w.noninteractive.fg_stroke = Stroke::new(1.0, INCHIOSTRO);
    w.noninteractive.corner_radius = tondo;
    for (stato, fondo) in [(&mut w.inactive, VELO), (&mut w.hovered, VELO), (&mut w.active, PREMUTO_VELO), (&mut w.open, VELO)] {
        stato.bg_fill = fondo;
        stato.weak_bg_fill = fondo;
        stato.bg_stroke = Stroke::NONE;
        stato.fg_stroke = Stroke::new(1.5, INCHIOSTRO);
        stato.corner_radius = tondo;
        stato.expansion = 0.0;
    }
    v
}

/// Lo stile di egui col tema: i colori di [`visuals`], i testi a 16 dp come il corpo del CSS, i
/// bersagli da 48 dp. Il lampeggio del cursore resta com'era (le prove a schermo lo spengono).
pub fn applica_stile(st: &mut Style) {
    let lampeggio = st.visuals.text_cursor.blink;
    st.visuals = visuals();
    st.visuals.text_cursor.blink = lampeggio;
    for (stile, font) in [
        (TextStyle::Small, FontId::proportional(13.0)),
        (TextStyle::Body, FontId::proportional(16.0)),
        (TextStyle::Button, FontId::proportional(16.0)),
        (TextStyle::Heading, FontId::proportional(22.0)),
        (TextStyle::Monospace, FontId::monospace(14.0)),
    ] {
        st.text_styles.insert(stile, font);
    }
    let s = &mut st.spacing;
    s.item_spacing = vec2(SPAZIO, SPAZIO);
    s.button_padding = vec2(16.0, 10.0);
    s.interact_size.y = PULSANTE;
    s.menu_margin = Margin::symmetric(0, 8);
    s.window_margin = Margin::same(24);
}

/// Installa il tema: caratteri, tema chiaro e stile. I caratteri valgono dal passo dopo; fino ad
/// allora i componenti ripiegano sul proporzionale di egui (vedi [`Carattere::font`]).
pub fn installa(ctx: &Context) {
    installa_con(ctx, &[]);
}

/// Come [`installa`], coi caratteri `altri` in più (vedi [`caratteri_con`]).
pub fn installa_con(ctx: &Context, altri: &[(&'static str, &'static [u8])]) {
    ctx.set_fonts(caratteri_con(altri));
    ctx.set_theme(ThemePreference::Light);
    ctx.style_mut_of(Theme::Light, applica_stile);
}

#[cfg(test)]
mod test {
    use super::*;

    /// I colori di `:root` in stile.css, `--nome: #RRGGBB`.
    fn colori_del_bozzetto() -> Vec<(String, Color32)> {
        let css = include_str!("../../../design/bozzetti/stile.css");
        let radice = css.split(":root {").nth(1).and_then(|r| r.split('}').next()).expect(":root in stile.css");
        radice
            .lines()
            .filter_map(|riga| {
                let (nome, resto) = riga.trim().strip_prefix("--")?.split_once(':')?;
                let hex = resto.trim().strip_prefix('#')?.get(..6)?;
                let v = u32::from_str_radix(hex, 16).ok()?;
                Some((nome.to_string(), Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)))
            })
            .collect()
    }

    #[test]
    fn i_colori_sono_quelli_del_bozzetto() {
        let nostri = [
            ("carta", CARTA),
            ("foglio", FOGLIO),
            ("inchiostro", INCHIOSTRO),
            ("matita", MATITA),
            ("filo", FILO),
            ("velo", VELO),
            ("blu", BLU),
            ("blu-tenue", BLU_TENUE),
            ("rosso", ROSSO),
            ("rosso-tenue", ROSSO_TENUE),
            ("ocra", OCRA),
            ("ocra-tenue", OCRA_TENUE),
            ("scuro", SCURO),
        ];
        let bozzetto = colori_del_bozzetto();
        assert_eq!(bozzetto.len(), nostri.len(), "colori nel bozzetto: {bozzetto:?}");
        for (nome, colore) in nostri {
            let atteso = bozzetto.iter().find(|(n, _)| n == nome).map(|(_, c)| *c);
            assert_eq!(atteso, Some(colore), "--{nome}");
        }
    }

    #[test]
    fn le_ombre_sfumano_piu_del_css() {
        // Il CSS sfuma con una gaussiana, egui in linea: la sfumatura di egui è più larga.
        for ombra in [OMBRA, OMBRA_FLOTTANTE, OMBRA_NUOVO, OMBRA_MENU, OMBRA_CONTESTO, OMBRA_FOGLIO, OMBRA_ACCESO] {
            for strato in ombra.iter().filter(|s| s.color != Color32::TRANSPARENT) {
                assert!(strato.blur >= 3 && strato.color.a() > 0, "{strato:?}");
            }
        }
        assert_eq!(OMBRA[1].offset, [0, 6]);
        assert_eq!(OMBRA_FOGLIO[0].offset, [0, -8], "l'ombra del foglio dal fondo va verso l'alto");
    }

    #[test]
    fn i_caratteri_tengono_il_ripiego_delle_emoji() {
        let d = caratteri();
        let proporzionale = &d.families[&FontFamily::Proportional];
        assert_eq!(proporzionale[0], "atkinson", "{proporzionale:?}");
        for emoji in ["NotoEmoji-Regular", "emoji-icon-font"] {
            assert!(proporzionale.iter().any(|n| n == emoji), "manca {emoji}: {proporzionale:?}");
        }
        for c in [Carattere::Grassetto, Carattere::Titolo, Carattere::Icona, Carattere::IconaPiena] {
            let nomi = &d.families[&c.famiglia()];
            assert!(nomi.iter().all(|n| d.font_data.contains_key(n)), "{c:?}: {nomi:?}");
        }
        assert_eq!(d.families[&Carattere::Grassetto.famiglia()][0], "atkinson-grassetto");
        assert_eq!(d.families[&Carattere::Titolo.famiglia()][0], "fraunces");
    }

    /// Il glifo del primo carattere di `testo` nell'atlante: due caratteri disegnati uguali hanno
    /// lo stesso.
    fn glifo(ctx: &egui::Context, testo: &str, font: FontId) -> Option<([u16; 2], egui::Vec2)> {
        ctx.fonts_mut(|f| {
            let uv = f.layout_no_wrap(testo.to_string(), font, Color32::WHITE).rows[0].glyphs.first()?.uv_rect;
            Some((uv.min, uv.size))
        })
    }

    /// Le icone scelte (design/icone-scelte.txt): nome e carattere.
    fn icone_scelte() -> Vec<(String, String)> {
        include_str!("../../../design/icone-scelte.txt")
            .lines()
            .filter_map(|riga| {
                let (nome, codice) = riga.split_once(' ')?;
                let c = char::from_u32(u32::from_str_radix(codice.trim(), 16).ok()?)?;
                Some((nome.to_string(), c.to_string()))
            })
            .collect()
    }

    fn contesto_col_tema() -> egui::Context {
        let ctx = egui::Context::default();
        installa(&ctx);
        // I caratteri nuovi valgono dal passo dopo.
        for _ in 0..2 {
            ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
        }
        ctx
    }

    #[test]
    fn prima_del_tema_i_caratteri_ripiegano_senza_panico() {
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
        assert!(!Carattere::Titolo.pronto(&ctx));
        assert_eq!(Carattere::Titolo.font(&ctx, 20.0), FontId::proportional(20.0));
        assert_eq!(Carattere::Icona.font(&ctx, 24.0), FontId::proportional(24.0));
        // Nel passo in cui si installa, i caratteri non ci sono ancora.
        installa(&ctx);
        assert_eq!(Carattere::Grassetto.font(&ctx, 16.0), FontId::proportional(16.0));
    }

    #[test]
    fn col_tema_ci_sono_le_famiglie_le_icone_e_le_emoji() {
        let ctx = contesto_col_tema();
        for c in [Carattere::Grassetto, Carattere::Titolo, Carattere::Icona, Carattere::IconaPiena] {
            assert!(c.pronto(&ctx), "{c:?}");
            assert_eq!(c.font(&ctx, 20.0).family, c.famiglia());
        }
        // Ogni icona della tabella si disegna, vuota e piena: un carattere che manca avrebbe il
        // glifo vuoto.
        let manca = glifo(&ctx, "A", Carattere::Icona.font(&ctx, 24.0));
        let scelte = icone_scelte();
        assert!(scelte.len() > 150, "icone scelte: {}", scelte.len());
        for (nome, icona) in &scelte {
            for c in [Carattere::Icona, Carattere::IconaPiena] {
                let g = glifo(&ctx, icona, c.font(&ctx, 24.0));
                assert!(g != manca && g.is_some_and(|g| g.1.x > 0.0), "{nome} in {c:?}: {g:?} {manca:?}");
            }
        }
        // La tabella delle costanti viene dallo stesso elenco.
        assert_eq!(crate::icone::ADD, "\u{e145}");
        assert_eq!(crate::icone::FORMAT_BOLD, "\u{e238}");
        // Le icone che l'app usa oggi dai caratteri di egui (☰, ↩, ⬆, ❌) restano, nel ripiego.
        let quadratino = glifo(&ctx, "\u{10FFFD}", FontId::proportional(20.0));
        for c in ["☰", "↩", "↪", "⬆", "⬇", "❌", "➡"] {
            assert_ne!(glifo(&ctx, c, FontId::proportional(20.0)), quadratino, "{c}");
        }
    }

    #[test]
    fn lo_stile_e_chiaro_come_la_carta() {
        let ctx = contesto_col_tema();
        assert_eq!(ctx.theme(), egui::Theme::Light);
        let st = ctx.global_style();
        assert!(!st.visuals.dark_mode);
        assert_eq!(st.visuals.panel_fill, CARTA);
        assert_eq!(st.visuals.window_fill, FOGLIO);
        assert_eq!(st.visuals.widgets.noninteractive.fg_stroke.color, INCHIOSTRO);
        assert_eq!(st.visuals.weak_text_color(), MATITA);
        assert_eq!(st.visuals.selection.bg_fill, SELEZIONE);
        assert_eq!(st.visuals.text_cursor.stroke.color, BLU);
        assert_eq!(st.text_styles[&egui::TextStyle::Body], FontId::proportional(16.0));
        assert!(st.spacing.interact_size.y >= PULSANTE, "bersagli da dito");
    }

    #[test]
    fn lo_stile_non_riaccende_il_cursore_che_lampeggia() {
        // Le prove a schermo lo spengono: lo stile del tema non lo riaccende.
        let mut st = egui::Style::default();
        st.visuals.text_cursor.blink = false;
        applica_stile(&mut st);
        assert!(!st.visuals.text_cursor.blink);
    }
}
