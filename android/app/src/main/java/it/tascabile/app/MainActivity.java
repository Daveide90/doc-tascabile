package it.tascabile.app;

import android.app.KeyguardManager;
import android.app.Presentation;
import android.content.ActivityNotFoundException;
import android.content.ClipData;
import android.content.ClipDescription;
import android.content.ClipboardManager;
import android.content.Intent;
import android.database.Cursor;
import android.graphics.Bitmap;
import android.graphics.Color;
import android.hardware.biometrics.BiometricManager;
import android.hardware.biometrics.BiometricPrompt;
import android.hardware.display.DisplayManager;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.CancellationSignal;
import android.os.Handler;
import android.os.Looper;
import android.os.PersistableBundle;
import android.provider.MediaStore;
import android.provider.OpenableColumns;
import android.text.InputType;
import android.util.DisplayMetrics;
import android.view.Display;
import android.view.KeyEvent;
import android.view.View;
import android.view.Window;
import android.view.WindowManager;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputMethodManager;
import android.widget.ImageView;

import androidx.annotation.NonNull;
import androidx.core.content.FileProvider;
import androidx.core.graphics.Insets;
import androidx.core.view.ViewCompat;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsAnimationCompat;
import androidx.core.view.WindowInsetsCompat;
import androidx.core.view.WindowInsetsControllerCompat;

import com.google.androidgamesdk.GameActivity;
import com.google.androidgamesdk.gametextinput.State;

import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;

/**
 * Unica classe Java di Tascabile. Tutto il resto è Rust (libtascabile.so).
 * Qui: tastiera (stato versionato), insets, "Apri con", condivisione, appunti, la foto della
 * firma, la presentazione delle Slide, chiusura sicura.
 */
public class MainActivity extends GameActivity {
    static {
        System.loadLibrary("tascabile");
    }

    /** Autorità del FileProvider del manifest: condivide solo getCacheDir()/condivisi/. */
    private static final String AUTORITA_CONDIVISIONE = "it.tascabile.app.condivisione";

    // Implementati in Rust (app/src/android_main.rs). Chiamati sul thread principale.
    private static native void nativeImeState(String text, int selStart, int selEnd,
                                              int compStart, int compEnd, long version);
    private static native void nativeInsets(int left, int top, int right, int bottom,
                                            int imeBottom, int imeVisible, int finale);
    private static native void nativeNewIntent();
    /** Una riga nel registro della diagnosi: mai testo dell'utente, nomi di file o percorsi. */
    private static native void nativeDiagnosi(String evento);
    /**
     * L'esito di una richiesta di foto: il suo numero; 1 pronta nella cartella della richiesta
     * (per più foto dalla galleria, quante), 0 annullata, -1 errore; quante ne erano state scelte.
     */
    private static native void nativeFotoFirma(int richiesta, int esito, int scelte);
    private static native void nativeImpronta(int richiesta, int esito);
    private static native void nativeSchermoEsterno(int larghezza, int altezza);

    /**
     * Le richieste di startActivityForResult per le foto (tascabileFotoFirma), una per fonte: la
     * fotocamera (fonte 0), un'immagine scelta (1), un PDF scelto da unire (2), più foto per la
     * scansione (3, RICHIESTA_IMMAGINI). Il codice meno RICHIESTA_FOTO è la fonte.
     */
    private static final int RICHIESTA_FOTO = 41;
    private static final int RICHIESTA_IMMAGINE = 42;
    private static final int RICHIESTA_PDF = 43;
    /** Un'immagine scelta più grande di così non si copia: Rust la ridurrebbe comunque. */
    private static final long MASSIMO_IMMAGINE = 64L << 20;
    /** Un PDF scelto più grande di così non si copia: lo stesso tetto dei PDF da aprire. */
    private static final long MASSIMO_PDF = 200L << 20;
    /** Le foto delle pagine di una scansione scelte insieme (fonte 3), al più MASSIMO_PAGINE. */
    private static final int RICHIESTA_IMMAGINI = 44;
    private static final int MASSIMO_PAGINE = 30;
    /**
     * Le richieste di foto: il numero che manda Rust con tascabileFotoFirma, che torna con
     * l'esito, così un esito vecchio non risponde a una richiesta nuova. Per ogni fonte quello con
     * cui è partita l'ultima volta: l'esito della fotocamera torna col numero della fotocamera
     * anche se nel frattempo è partita un'altra richiesta. E l'ultimo in assoluto: una copia che
     * scopre di essere superata si ferma. Restano anche se Android toglie il processo mentre la
     * fotocamera o la galleria sono aperte. Solo thread principale, salvo richiestaFoto.
     */
    private final int[] richiestaDellaFonte = new int[4];
    private volatile int richiestaFoto = 0;
    private static final String RICHIESTA_SALVATA = "tascabile.richiestaFoto";
    private static final String RICHIESTE_SALVATE = "tascabile.richiesteDelleFonti";

    /** Versione dell'ultimo stato dato alla tastiera da Rust. Solo thread principale. */
    private long imeVersion = 0;

    /** I tipi di tastiera di tascabileTipoTastiera (TipoTastiera::codice in Rust). */
    private static final int TASTIERA_TESTO = 0;
    private static final int TASTIERA_NUMERI = 1;
    private static final int TASTIERA_PASSWORD = 2;

    /**
     * NO_PERSONALIZED_LEARNING: la tastiera non impara le parole battute qui (cognomi, termini
     * clinici, password) e non le propone in altre app. NO_ENTER_ACTION: l'Invio resta un a capo,
     * che Rust riconosce come Invio in ogni campo.
     */
    private static final int OPZIONI_TASTIERA = EditorInfo.IME_ACTION_NONE
            | EditorInfo.IME_FLAG_NO_ENTER_ACTION
            | EditorInfo.IME_FLAG_NO_FULLSCREEN
            | EditorInfo.IME_FLAG_NO_EXTRACT_UI
            | EditorInfo.IME_FLAG_NO_PERSONALIZED_LEARNING;

    /**
     * L'inputType di ogni tipo. MULTI_LINE resta in tutti: GameTextInput, senza, filtra gli a capo
     * e manda l'Invio come azione dell'editor, che a Rust non arriva. Il testo ha correzioni e
     * maiuscole a inizio frase; le cifre la tastiera dei numeri; la password niente suggerimenti,
     * correzioni né maiuscole (cambierebbero la password).
     */
    private static int tipoDiInput(int tipo) {
        switch (tipo) {
            case TASTIERA_NUMERI:
                return InputType.TYPE_CLASS_NUMBER | InputType.TYPE_TEXT_FLAG_MULTI_LINE;
            case TASTIERA_PASSWORD:
                return InputType.TYPE_CLASS_TEXT
                        | InputType.TYPE_TEXT_VARIATION_PASSWORD
                        | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
                        | InputType.TYPE_TEXT_FLAG_MULTI_LINE;
            default:
                return InputType.TYPE_CLASS_TEXT
                        | InputType.TYPE_TEXT_FLAG_MULTI_LINE
                        | InputType.TYPE_TEXT_FLAG_AUTO_CORRECT
                        | InputType.TYPE_TEXT_FLAG_CAP_SENTENCES;
        }
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        // Editor di testo lungo. GameActivity legge questo EditorInfo dentro super.onCreate().
        EditorInfo info = getImeEditorInfo();
        info.inputType = tipoDiInput(TASTIERA_TESTO);
        info.imeOptions = OPZIONI_TASTIERA;
        info.actionId = EditorInfo.IME_ACTION_NONE;
        // Ricreata dopo che Android ha tolto il processo, o rilanciata dalle app recenti: l'Intent
        // è quello con cui era nata (un vecchio «Apri con» di WhatsApp), non una richiesta nuova,
        // e il primo take_open_request di Rust non deve riaprirlo saltando il recupero della Home.
        // Prima di super.onCreate(), che fa partire android_main; il componente resta, perché
        // GameActivity lo usa per trovare la libreria.
        if (savedInstanceState != null) {
            richiestaFoto = savedInstanceState.getInt(RICHIESTA_SALVATA, 0);
            int[] salvate = savedInstanceState.getIntArray(RICHIESTE_SALVATE);
            if (salvate != null && salvate.length == richiestaDellaFonte.length) {
                System.arraycopy(salvate, 0, richiestaDellaFonte, 0, salvate.length);
            }
        }
        Intent nata = getIntent();
        boolean dalleRecenti = nata != null && (nata.getFlags() & Intent.FLAG_ACTIVITY_LAUNCHED_FROM_HISTORY) != 0;
        if (nata != null && (savedInstanceState != null || dalleRecenti)) {
            Intent avvio = new Intent(Intent.ACTION_MAIN);
            avvio.setComponent(nata.getComponent());
            setIntent(avvio);
        }
        super.onCreate(savedInstanceState);

        // Nelle app recenti niente miniatura della pagina aperta (Android 13 e successivi).
        // FLAG_SECURE solo con la modalità riservata delle Impostazioni (tascabileRiservata):
        // spenta di base, perché gli screenshot servono per le prove e per mostrare un problema.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            setRecentsScreenshotEnabled(false);
        }

        // Altezza della tastiera anche durante l'animazione, per la barra di formato.
        if (mSurfaceView != null) {
            ViewCompat.setWindowInsetsAnimationCallback(mSurfaceView,
                    new WindowInsetsAnimationCompat.Callback(
                            WindowInsetsAnimationCompat.Callback.DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
                        @NonNull
                        @Override
                        public WindowInsetsCompat onProgress(@NonNull WindowInsetsCompat insets,
                                @NonNull List<WindowInsetsAnimationCompat> running) {
                            sendInsets(insets, false);
                            return insets;
                        }
                    });
        }
    }

    /** La tastiera ha cambiato testo, selezione o composizione. */
    @Override
    public void stateChanged(State newState, boolean dismissed) {
        // Niente super.stateChanged: porterebbe lo stato anche ad android-activity, che lo copia
        // in un buffer da 64 KiB e lo rilegge per intero (un incolla enorme lo farebbe leggere
        // oltre la fine). winit lo ignora comunque: lo stato arriva a Rust solo da qui.
        nativeImeState(newState.text, newState.selectionStart, newState.selectionEnd,
                newState.composingRegionStart, newState.composingRegionEnd, imeVersion);
    }

    /**
     * Chiamato da Rust sul thread principale: nuovo testo per la tastiera.
     * Selezione in unità UTF-16 (indici di String Java).
     */
    public void tascabileSetIme(String text, int selStart, int selEnd, long version) {
        imeVersion = version;
        EditorInfo info = getImeEditorInfo();
        info.initialSelStart = selStart;
        info.initialSelEnd = selEnd;
        setTextInputState(new State(text, selStart, selEnd, -1, -1));
    }

    /**
     * Chiamato da Rust sul thread principale, quando il fuoco passa a un campo che vuole un'altra
     * tastiera (TipoTastiera): l'EditorInfo di GameTextInput cambia e la tastiera riparte con
     * restartInput (quello che fa InputConnection.restartInput). Un errore va nella diagnosi col
     * solo tipo dell'eccezione.
     */
    public void tascabileTipoTastiera(int tipo) {
        try {
            setImeEditorInfoFields(tipoDiInput(tipo), EditorInfo.IME_ACTION_NONE, OPZIONI_TASTIERA);
            InputMethodManager imm = (InputMethodManager) getSystemService(INPUT_METHOD_SERVICE);
            if (imm != null && mSurfaceView != null) {
                imm.restartInput(mSurfaceView);
            }
        } catch (RuntimeException e) {
            nativeDiagnosi("tastiera: tipo non cambiato (" + e.getClass().getSimpleName() + ")");
        }
    }

    @Override
    public WindowInsetsCompat onApplyWindowInsets(View v, WindowInsetsCompat insets) {
        WindowInsetsCompat result = super.onApplyWindowInsets(v, insets);
        sendInsets(insets, true);
        return result;
    }

    /**
     * Insets a Rust. finale: da onApplyWindowInsets, che prima di un'animazione della tastiera
     * dà già lo stato di fine animazione (documentazione di WindowInsetsAnimation.Callback
     * .onPrepare); se no è un frame dell'animazione (onProgress). La diagnosi scrive solo i primi.
     */
    private static void sendInsets(WindowInsetsCompat insets, boolean finale) {
        Insets bars = insets.getInsets(WindowInsetsCompat.Type.systemBars()
                | WindowInsetsCompat.Type.displayCutout());
        Insets ime = insets.getInsets(WindowInsetsCompat.Type.ime());
        boolean imeVisible = insets.isVisible(WindowInsetsCompat.Type.ime());
        nativeInsets(bars.left, bars.top, bars.right, bars.bottom, ime.bottom, imeVisible ? 1 : 0,
                finale ? 1 : 0);
    }

    /**
     * Chiamato da Rust sul thread di android_main: il nome vero del file di un "Apri con"
     * (OpenableColumns.DISPLAY_NAME), o null. Il nome non va mai nella diagnosi.
     */
    public String tascabileNomeUri(Uri uri) {
        if (uri == null || !"content".equals(uri.getScheme())) return null;
        String[] colonne = {OpenableColumns.DISPLAY_NAME};
        try (Cursor c = getContentResolver().query(uri, colonne, null, null, null)) {
            if (c != null && c.moveToFirst()) {
                int i = c.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                if (i >= 0 && !c.isNull(i)) return c.getString(i);
            }
        } catch (RuntimeException e) {
            nativeDiagnosi("apri con: nome del file non letto (" + e.getClass().getSimpleName() + ")");
        }
        return null;
    }

    /** Secondo "Apri con" mentre l'app è aperta (launchMode singleTask). */
    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        nativeNewIntent();
    }

    /**
     * Chiamato da Rust sul thread principale: condivide un file col foglio di sistema.
     * Il file deve stare in getCacheDir()/condivisi/ (res/xml/percorsi_condivisi.xml).
     * Alla diagnosi va solo il tipo dell'errore: i messaggi delle eccezioni hanno il percorso.
     */
    public void tascabileCondividiFile(String percorso, String mime, String titolo) {
        try {
            Uri uri = FileProvider.getUriForFile(this, AUTORITA_CONDIVISIONE, new File(percorso));
            Intent invio = new Intent(Intent.ACTION_SEND);
            invio.setType(mime);
            invio.putExtra(Intent.EXTRA_STREAM, uri);
            // L'URI anche nel ClipData: Intent.createChooser copia ClipData e permesso nel
            // foglio di sistema, che così può leggere il file per l'anteprima, non solo l'app
            // scelta. Con il solo EXTRA_STREAM il permesso arriva soltanto a quest'ultima.
            invio.setClipData(ClipData.newRawUri(null, uri));
            invio.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
            startActivity(Intent.createChooser(invio, titolo));
        } catch (IllegalArgumentException e) {
            // Rust lo controlla già prima (righe::dentro_condivisi): se arriva qui, i due
            // controlli non vanno d'accordo.
            nativeDiagnosi("condividi file: fuori dalla cartella condivisi (IllegalArgumentException)");
        } catch (ActivityNotFoundException e) {
            nativeDiagnosi("condividi file: nessuna app per condividere (ActivityNotFoundException)");
        }
    }

    /**
     * Chiamato da Rust sul thread principale: apre un link esterno di un PDF (pagina web o mail,
     * già controllato e confermato in Rust) nell'app di sistema che lo sa aprire, di solito il
     * browser: la pagina la apre un'altra app (la rete di Tascabile serve solo al timestamp).
     * Solo app che si dichiarano apribili da un link (CATEGORY_BROWSABLE). Alla diagnosi mai
     * l'indirizzo.
     */
    public void tascabileApriIndirizzo(String indirizzo) {
        try {
            Intent vista = new Intent(Intent.ACTION_VIEW, Uri.parse(indirizzo));
            vista.addCategory(Intent.CATEGORY_BROWSABLE);
            startActivity(vista);
        } catch (ActivityNotFoundException e) {
            nativeDiagnosi("link esterno: nessuna app per aprirlo (ActivityNotFoundException)");
        } catch (RuntimeException e) {
            nativeDiagnosi("link esterno: non aperto (" + e.getClass().getSimpleName() + ")");
        }
    }

    /**
     * Il primo file della richiesta: getCacheDir()/foto/scelte-richiesta/pagina-1 (crate
     * platform, file_della_pagina), con la sua cartella. La fotocamera ci scrive tramite il
     * FileProvider (res/xml/percorsi_condivisi.xml); un'immagine o un PDF scelto si copia lì.
     * Rust lo legge e lo cancella.
     */
    private File fileDellaRichiesta(int richiesta) {
        File cartella = cartellaDellaRichiesta(richiesta);
        cartella.mkdirs();
        return new File(cartella, "pagina-1");
    }

    /**
     * Chiamato da Rust sul thread principale: una foto, ognuna nella cartella della sua richiesta
     * (cartellaDellaRichiesta). Fonte 0: la fotocamera di sistema (ACTION_IMAGE_CAPTURE), che
     * scrive nel primo file; Tascabile non chiede il permesso della fotocamera (senza dichiararlo,
     * l'Intent non lo vuole). Fonte 1: un'immagine scelta (ACTION_GET_CONTENT), copiata lì. Fonte
     * 2: un PDF scelto (ACTION_GET_CONTENT, application/pdf) da unire, copiato lì. Fonte 3: le
     * foto delle pagine di una scansione, anche più d'una (EXTRA_ALLOW_MULTIPLE), copiate nei file
     * pagina-1, pagina-2… nell'ordine. L'esito a Rust con nativeFotoFirma: la richiesta, l'esito
     * (per la fonte 3 quante foto) e quante ne erano state scelte; alla diagnosi solo il tipo
     * dell'errore.
     */
    public void tascabileFotoFirma(int fonte, int richiesta) {
        richiestaFoto = richiesta;
        if (fonte >= 0 && fonte < richiestaDellaFonte.length) richiestaDellaFonte[fonte] = richiesta;
        try {
            if (fonte == 0) {
                Uri uri = FileProvider.getUriForFile(this, AUTORITA_CONDIVISIONE, fileDellaRichiesta(richiesta));
                Intent scatta = new Intent(MediaStore.ACTION_IMAGE_CAPTURE);
                scatta.putExtra(MediaStore.EXTRA_OUTPUT, uri);
                // Come nella condivisione: il permesso anche dal ClipData, per l'app della foto.
                scatta.setClipData(ClipData.newRawUri(null, uri));
                scatta.addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION | Intent.FLAG_GRANT_READ_URI_PERMISSION);
                startActivityForResult(scatta, RICHIESTA_FOTO);
            } else if (fonte == 2) {
                Intent scegli = new Intent(Intent.ACTION_GET_CONTENT);
                scegli.setType("application/pdf");
                scegli.addCategory(Intent.CATEGORY_OPENABLE);
                startActivityForResult(scegli, RICHIESTA_PDF);
            } else if (fonte == 3) {
                Intent scegli = new Intent(Intent.ACTION_GET_CONTENT);
                scegli.setType("image/*");
                scegli.putExtra(Intent.EXTRA_MIME_TYPES, new String[] {"image/jpeg", "image/png"});
                scegli.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true);
                scegli.addCategory(Intent.CATEGORY_OPENABLE);
                startActivityForResult(scegli, RICHIESTA_IMMAGINI);
            } else {
                Intent scegli = new Intent(Intent.ACTION_GET_CONTENT);
                scegli.setType("image/*");
                scegli.putExtra(Intent.EXTRA_MIME_TYPES, new String[] {"image/jpeg", "image/png"});
                scegli.addCategory(Intent.CATEGORY_OPENABLE);
                startActivityForResult(scegli, RICHIESTA_IMMAGINE);
            }
        } catch (ActivityNotFoundException e) {
            nativeDiagnosi("foto della firma: nessuna app (ActivityNotFoundException)");
            cancellaCartella(cartellaDellaRichiesta(richiesta));
            nativeFotoFirma(richiesta, -1, 0);
        } catch (RuntimeException e) {
            nativeDiagnosi("foto della firma: non chiesta (" + e.getClass().getSimpleName() + ")");
            cancellaCartella(cartellaDellaRichiesta(richiesta));
            nativeFotoFirma(richiesta, -1, 0);
        }
    }

    @Override
    protected void onSaveInstanceState(@NonNull Bundle outState) {
        super.onSaveInstanceState(outState);
        outState.putInt(RICHIESTA_SALVATA, richiestaFoto);
        outState.putIntArray(RICHIESTE_SALVATE, richiestaDellaFonte.clone());
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        int fonte = requestCode - RICHIESTA_FOTO;
        if (fonte < 0 || fonte >= richiestaDellaFonte.length) return;
        // Il numero con cui è partita questa fonte, non quello di adesso: se nel frattempo è
        // partita un'altra richiesta, Rust lascia questo esito e butta la sua cartella.
        final int richiesta = richiestaDellaFonte[fonte];
        if (requestCode == RICHIESTA_FOTO) {
            boolean pronta = resultCode == RESULT_OK && fileDellaRichiesta(richiesta).length() > 0;
            if (!pronta) cancellaCartella(cartellaDellaRichiesta(richiesta));
            nativeFotoFirma(richiesta, pronta ? 1 : 0, 1);
        } else if (requestCode == RICHIESTA_IMMAGINE || requestCode == RICHIESTA_PDF) {
            Uri uri = data == null ? null : data.getData();
            if (resultCode != RESULT_OK || uri == null) {
                nativeFotoFirma(richiesta, 0, 0);
                return;
            }
            long massimo = requestCode == RICHIESTA_PDF ? MASSIMO_PDF : MASSIMO_IMMAGINE;
            // La copia su un thread suo: il file può venire da una cartella in rete.
            new Thread(() -> {
                boolean copiata = copia(uri, fileDellaRichiesta(richiesta), massimo);
                if (!copiata) cancellaCartella(cartellaDellaRichiesta(richiesta));
                nativeFotoFirma(richiesta, copiata ? 1 : -1, 1);
            }, "tascabile-foto").start();
        } else if (requestCode == RICHIESTA_IMMAGINI) {
            // Più foto scelte arrivano nel ClipData; una sola, a volte, solo in getData.
            List<Uri> scelte = new ArrayList<>();
            int tutte = 0;
            if (resultCode == RESULT_OK && data != null) {
                ClipData clip = data.getClipData();
                if (clip != null) {
                    for (int i = 0; i < clip.getItemCount(); i++) {
                        Uri u = clip.getItemAt(i).getUri();
                        if (u == null) continue;
                        tutte++;
                        if (scelte.size() < MASSIMO_PAGINE) scelte.add(u);
                    }
                } else if (data.getData() != null) {
                    scelte.add(data.getData());
                    tutte = 1;
                }
            }
            if (scelte.isEmpty()) {
                nativeFotoFirma(richiesta, 0, 0);
                return;
            }
            final int quante = tutte;
            new Thread(() -> nativeFotoFirma(richiesta, copiaPagine(scelte, richiesta), quante), "tascabile-foto").start();
        }
    }

    /**
     * Copia il file scelto (un'immagine o un PDF) in file, fino a massimo byte: vero se è
     * riuscita; se no il file se ne va.
     */
    private boolean copia(Uri uri, File file, long massimo) {
        // Solo i content:// di altre app, come per «Apri con»: un'app scelta come galleria potrebbe
        // rispondere con un file:// o col nostro FileProvider e farci copiare un nostro file privato
        // (revisione del 31/10, punto 5). getAuthority() è già decodificata. Un'autorità con
        // l'utente davanti («0@it.tascabile.app.condivisione») è la nostra lo stesso, e Android
        // toglie l'utente prima di aprirla: un'autorità con la «@» non si accetta mai (revisione
        // del 3/11, B1).
        String autorita = uri == null ? null : uri.getAuthority();
        if (uri == null || !"content".equals(uri.getScheme()) || autorita == null || autorita.indexOf('@') >= 0
                || AUTORITA_CONDIVISIONE.equalsIgnoreCase(autorita)) {
            file.delete();
            return false;
        }
        try (InputStream in = getContentResolver().openInputStream(uri);
             OutputStream out = new FileOutputStream(file)) {
            if (in == null) throw new IOException();
            byte[] pezzo = new byte[64 * 1024];
            long letti = 0;
            int n;
            while ((n = in.read(pezzo)) > 0) {
                letti += n;
                if (letti > massimo) throw new IOException();
                out.write(pezzo, 0, n);
            }
            return true;
        } catch (IOException | RuntimeException e) {
            nativeDiagnosi("file scelto: non copiato (" + e.getClass().getSimpleName() + ")");
            file.delete();
            return false;
        }
    }

    /**
     * La cartella delle foto della richiesta: getCacheDir()/foto/scelte-richiesta/ (crate
     * platform, cartella_delle_scelte). Una cartella per richiesta: due richieste non scrivono
     * mai sugli stessi file. Rust legge le foto e le cancella.
     */
    private File cartellaDellaRichiesta(int richiesta) {
        return new File(new File(getCacheDir(), "foto"), "scelte-" + richiesta);
    }

    /**
     * Copia le foto scelte per la scansione nei file pagina-1, pagina-2… della cartella della
     * richiesta, nell'ordine della scelta, saltando quelle che non si leggono: quante sono, o -1
     * se nessuna (e la cartella se ne va). Se nel frattempo è partita un'altra richiesta, questa
     * non serve più: si ferma e cancella quello che ha copiato.
     */
    private int copiaPagine(List<Uri> scelte, int richiesta) {
        File cartella = cartellaDellaRichiesta(richiesta);
        cartella.mkdirs();
        int copiate = 0;
        for (Uri uri : scelte) {
            if (richiesta != richiestaFoto) {
                cancellaCartella(cartella);
                return -1;
            }
            if (copia(uri, new File(cartella, "pagina-" + (copiate + 1)), MASSIMO_IMMAGINE)) copiate++;
        }
        if (copiate == 0) cancellaCartella(cartella);
        return copiate > 0 ? copiate : -1;
    }

    /** Una cartella di foto, coi suoi file. */
    private static void cancellaCartella(File cartella) {
        File[] dentro = cartella.listFiles();
        if (dentro != null) {
            for (File f : dentro) f.delete();
        }
        cartella.delete();
    }

    /** Chiamato da Rust sul thread principale: condivide un testo (per esempio la diagnosi). */
    public void tascabileCondividiTesto(String testo, String titolo) {
        Intent invio = new Intent(Intent.ACTION_SEND);
        invio.setType("text/plain");
        invio.putExtra(Intent.EXTRA_TEXT, testo);
        // Oggetto per chi la manda per posta.
        invio.putExtra(Intent.EXTRA_SUBJECT, titolo);
        try {
            startActivity(Intent.createChooser(invio, titolo));
        } catch (ActivityNotFoundException e) {
            nativeDiagnosi("condividi testo: nessuna app per condividere (ActivityNotFoundException)");
        }
    }

    /** Appunti di sistema (egui su Android ha solo appunti interni all'app). Thread principale. */
    public void tascabileCopy(String testo) {
        ClipboardManager cm = (ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
        if (cm == null) {
            nativeDiagnosi("appunti: servizio assente, niente copiato");
            return;
        }
        ClipData clip = ClipData.newPlainText("Tascabile", testo);
        // Contenuto sensibile: da Android 13 l'anteprima degli appunti che compare dopo la copia
        // non mostra il testo, che può essere quello di un documento clinico. È la chiave di
        // ClipDescription.EXTRA_IS_SENSITIVE (API 33), scritta per esteso perché la costante non
        // c'è prima della 13; le versioni prima della 13 la ignorano. setExtras è dell'API 24.
        PersistableBundle extra = new PersistableBundle();
        extra.putBoolean("android.content.extra.IS_SENSITIVE", true);
        segnoAppunti = new java.security.SecureRandom().nextLong();
        extra.putLong(SEGNO_APPUNTI, segnoAppunti);
        clip.getDescription().setExtras(extra);
        cm.setPrimaryClip(clip);
        principale.removeCallbacks(svuotaAppunti);
        principale.postDelayed(svuotaAppunti, DURATA_APPUNTI);
    }

    /**
     * La copia resta negli appunti al più due minuti, il tempo di incollarla in un'altra app: poi
     * chiunque li legga (un'app in primo piano, la tastiera) vedrebbe il testo di un documento
     * (revisione del 31/10, punto 6). In primo piano si controlla che sia ancora la nostra copia;
     * in secondo piano Android non lo lascia vedere e si svuota lo stesso, come fanno i gestori di
     * password.
     */
    private static final long DURATA_APPUNTI = 2 * 60 * 1000;
    private static final String SEGNO_APPUNTI = "it.tascabile.app.segno";
    private final Handler principale = new Handler(Looper.getMainLooper());
    private volatile long segnoAppunti;
    private final Runnable svuotaAppunti = () -> {
        ClipboardManager cm = (ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
        if (cm == null) return;
        ClipDescription d = cm.getPrimaryClipDescription();
        if (d != null && (d.getExtras() == null || d.getExtras().getLong(SEGNO_APPUNTI, 0) != segnoAppunti)) return;
        cm.clearPrimaryClip();
    };

    /** Al più tanti caratteri incollati: un testo vero, non un file. */
    private static final int MASSIMO_INCOLLA = 1_000_000;

    /**
     * Testo degli appunti, o null. Chiamato da Rust sul thread di android_main, che aspetta la
     * risposta: getPrimaryClip è una chiamata Binder e non vuole un Looper (l'Handler del
     * ClipboardManager è quello del thread principale e serve solo agli ascoltatori dei cambi).
     * Da Android 10 risponde null se l'app non ha il fuoco.
     */
    public String tascabilePaste() {
        ClipboardManager cm = (ClipboardManager) getSystemService(CLIPBOARD_SERVICE);
        if (cm == null) return null;
        ClipData clip = cm.getPrimaryClip();
        if (clip == null || clip.getItemCount() == 0) return null;
        // Solo il testo vero: coerceToText leggeva anche un file messo negli appunti da un'altra
        // app, senza tetto e sul thread che disegna (revisione del 31/10, punto 6).
        CharSequence t = clip.getItemAt(0).getText();
        if (t == null) return null;
        if (t.length() > MASSIMO_INCOLLA) {
            nativeDiagnosi("appunti: testo oltre il milione di caratteri, non incollato");
            return null;
        }
        return t.toString();
    }

    /** La presentazione delle Slide è accesa (lo dice Rust): i tasti del volume la comandano. */
    private volatile boolean presentazione = false;

    /**
     * Chiamato da Rust sul thread principale quando una presentazione comincia o finisce. Mentre
     * c'è: lo schermo resta acceso, le barre di sistema si nascondono (tornano per un attimo
     * scorrendo dal bordo) e i tasti del volume vanno avanti e indietro.
     */
    /**
     * L'impronta, o il PIN, la sequenza o la password del telefono, con la finestra di sistema di
     * Android (BiometricPrompt): la disegna Android, e un pezzo dell'app che fosse stato bucato non
     * la può né falsare né saltare (roadmap della sicurezza, Fase 1). L'esito a Rust con
     * nativeImpronta: 1 riuscita, 0 annullata, -2 telefono senza blocco dello schermo, -1 errore.
     */
    public void tascabileImpronta(int richiesta, String titolo, String spiegazione) {
        runOnUiThread(() -> {
            KeyguardManager blocco = (KeyguardManager) getSystemService(KEYGUARD_SERVICE);
            if (blocco == null || !blocco.isDeviceSecure()) {
                nativeImpronta(richiesta, -2);
                return;
            }
            try {
                BiometricPrompt finestra = new BiometricPrompt.Builder(this)
                        .setTitle(titolo)
                        .setDescription(spiegazione)
                        .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG
                                | BiometricManager.Authenticators.DEVICE_CREDENTIAL)
                        .build();
                finestra.authenticate(new CancellationSignal(), getMainExecutor(), new BiometricPrompt.AuthenticationCallback() {
                    @Override
                    public void onAuthenticationSucceeded(BiometricPrompt.AuthenticationResult risultato) {
                        nativeImpronta(richiesta, 1);
                    }

                    @Override
                    public void onAuthenticationError(int codice, CharSequence testo) {
                        // Col PIN ammesso Android non mette il nostro pulsante «Annulla»: annullare è
                        // il gesto Indietro, o la finestra chiusa dal sistema.
                        boolean annullata = codice == BiometricPrompt.BIOMETRIC_ERROR_USER_CANCELED
                                || codice == BiometricPrompt.BIOMETRIC_ERROR_CANCELED;
                        boolean senzaBlocco = codice == BiometricPrompt.BIOMETRIC_ERROR_NO_DEVICE_CREDENTIAL;
                        nativeImpronta(richiesta, annullata ? 0 : (senzaBlocco ? -2 : -1));
                    }
                });
            } catch (RuntimeException e) {
                nativeImpronta(richiesta, -1);
            }
        });
    }

    /**
     * La modalità riservata delle Impostazioni (la dice Rust all'avvio e a ogni cambio): accesa,
     * la finestra è nera negli screenshot, nelle registrazioni dello schermo, nella condivisione
     * durante una videochiamata e, su Android 11 e 12, nella miniatura delle app recenti
     * (FLAG_SECURE); da Android 12 le finestre di altre app non stanno sopra la nostra, così non
     * fanno toccare il punto sbagliato su password e firma (setHideOverlayWindows).
     */
    public void tascabileRiservata(boolean accesa) {
        Window finestra = getWindow();
        if (accesa) {
            finestra.addFlags(WindowManager.LayoutParams.FLAG_SECURE);
        } else {
            finestra.clearFlags(WindowManager.LayoutParams.FLAG_SECURE);
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
            finestra.setHideOverlayWindows(accesa);
        }
    }

    public void tascabilePresentazione(boolean attiva) {
        presentazione = attiva;
        if (attiva) {
            ascoltaGliSchermi();
        } else {
            smettiDiAscoltare();
        }
        Window finestra = getWindow();
        WindowInsetsControllerCompat barre = WindowCompat.getInsetsController(finestra, finestra.getDecorView());
        if (attiva) {
            finestra.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            barre.setSystemBarsBehavior(WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
            barre.hide(WindowInsetsCompat.Type.systemBars());
        } else {
            finestra.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
            barre.show(WindowInsetsCompat.Type.systemBars());
        }
    }

    /** Lo schermo esterno della presentazione (cavo HDMI, Miracast) e l'immagine che ci si mostra. */
    private Presentation proiettore;
    private ImageView immagineProiettore;
    private DisplayManager.DisplayListener ascoltoSchermi;

    /** Mentre si presenta: uno schermo esterno attaccato riceve le slide, uno staccato si lascia. */
    private void ascoltaGliSchermi() {
        DisplayManager dm = (DisplayManager) getSystemService(DISPLAY_SERVICE);
        if (dm == null) return;
        if (ascoltoSchermi == null) {
            ascoltoSchermi = new DisplayManager.DisplayListener() {
                @Override public void onDisplayAdded(int id) { controllaSchermoEsterno(); }
                @Override public void onDisplayRemoved(int id) { controllaSchermoEsterno(); }
                @Override public void onDisplayChanged(int id) { controllaSchermoEsterno(); }
            };
            dm.registerDisplayListener(ascoltoSchermi, null);
        }
        controllaSchermoEsterno();
    }

    private void smettiDiAscoltare() {
        DisplayManager dm = (DisplayManager) getSystemService(DISPLAY_SERVICE);
        if (dm != null && ascoltoSchermi != null) {
            dm.unregisterDisplayListener(ascoltoSchermi);
        }
        ascoltoSchermi = null;
        chiudiProiettore();
        nativeSchermoEsterno(0, 0);
    }

    /**
     * Il primo schermo per le presentazioni (DISPLAY_CATEGORY_PRESENTATION: HDMI, Miracast, non lo
     * schermo del telefono): una Presentation nera con un'immagine adattata in mezzo; a Rust la
     * sua misura in pixel, o 0 × 0 se non c'è.
     */
    private void controllaSchermoEsterno() {
        DisplayManager dm = (DisplayManager) getSystemService(DISPLAY_SERVICE);
        Display[] esterni = dm == null ? new Display[0] : dm.getDisplays(DisplayManager.DISPLAY_CATEGORY_PRESENTATION);
        Display schermo = esterni.length > 0 ? esterni[0] : null;
        if (schermo == null) {
            chiudiProiettore();
            nativeSchermoEsterno(0, 0);
            return;
        }
        if (proiettore != null && proiettore.getDisplay().getDisplayId() == schermo.getDisplayId()) {
            return;
        }
        chiudiProiettore();
        Presentation p = new Presentation(this, schermo);
        ImageView immagine = new ImageView(p.getContext());
        immagine.setBackgroundColor(Color.BLACK);
        immagine.setScaleType(ImageView.ScaleType.FIT_CENTER);
        p.setContentView(immagine);
        try {
            p.show();
        } catch (RuntimeException e) {
            // WindowManager.InvalidDisplayException: lo schermo se n'è andato nel frattempo.
            nativeDiagnosi("schermo esterno: non si apre (" + e.getClass().getSimpleName() + ")");
            nativeSchermoEsterno(0, 0);
            return;
        }
        proiettore = p;
        immagineProiettore = immagine;
        DisplayMetrics misura = new DisplayMetrics();
        schermo.getRealMetrics(misura);
        nativeSchermoEsterno(misura.widthPixels, misura.heightPixels);
    }

    private void chiudiProiettore() {
        if (proiettore != null) {
            proiettore.dismiss();
        }
        proiettore = null;
        immagineProiettore = null;
    }

    /**
     * Chiamato da Rust sul thread principale: i pixel da mostrare sullo schermo esterno, RGBA
     * premoltiplicato, come la memoria di una Bitmap ARGB_8888.
     */
    public void tascabileProietta(byte[] pixel, int larghezza, int altezza) {
        if (immagineProiettore == null || pixel.length < larghezza * altezza * 4) return;
        Bitmap immagine = Bitmap.createBitmap(larghezza, altezza, Bitmap.Config.ARGB_8888);
        immagine.copyPixelsFromBuffer(ByteBuffer.wrap(pixel));
        immagineProiettore.setImageBitmap(immagine);
    }

    /**
     * Il «Timestamp con blockchain», l'unica cosa per cui l'app va in rete: una richiesta https (POST col corpo,
     * GET se il corpo è null), chiamata dal thread di lavoro del Rust, che aspetta. Dà lo stato in
     * due byte e poi la risposta: 0 senza collegamento, 1 se la risposta supera 10 000 byte. Solo
     * https, attese brevi, niente reindirizzamenti, niente cache; nessun registro, perché
     * l'indirizzo ha dentro l'impronta di un file.
     */
    public byte[] tascabileRete(String indirizzo, byte[] corpo) {
        if (!reteConsentita) return statoRete(0);
        HttpURLConnection c = null;
        java.util.Timer orologio = null;
        try {
            URL url = new URL(indirizzo);
            if (!ammesso(url)) return statoRete(0);
            c = (HttpURLConnection) url.openConnection();
            // Un tempo massimo per tutta la richiesta: setReadTimeout vale per ogni lettura, e una
            // rete che manda un byte ogni 14 secondi la teneva appesa per ore (revisione del 31/10).
            final HttpURLConnection daChiudere = c;
            orologio = new java.util.Timer("tascabile-rete", true);
            orologio.schedule(new java.util.TimerTask() {
                @Override
                public void run() {
                    daChiudere.disconnect();
                }
            }, DURATA_MASSIMA_RETE);
            c.setConnectTimeout(10000);
            c.setReadTimeout(15000);
            c.setInstanceFollowRedirects(false);
            c.setUseCaches(false);
            c.setRequestProperty("Accept", "application/vnd.opentimestamps.v1");
            c.setRequestProperty("User-Agent", "DocTascabile");
            if (corpo != null) {
                c.setRequestMethod("POST");
                c.setDoOutput(true);
                c.setFixedLengthStreamingMode(corpo.length);
                c.setRequestProperty("Content-Type", "application/x-www-form-urlencoded");
                try (OutputStream fuori = c.getOutputStream()) {
                    fuori.write(corpo);
                }
            }
            int codice = c.getResponseCode();
            if (codice != 200) return statoRete(codice);
            ByteArrayOutputStream letta = new ByteArrayOutputStream();
            try (InputStream dentro = c.getInputStream()) {
                byte[] pezzo = new byte[4096];
                int n;
                while ((n = dentro.read(pezzo)) != -1) {
                    letta.write(pezzo, 0, n);
                    if (letta.size() > 10000) return statoRete(1);
                }
            }
            byte[] risposta = letta.toByteArray();
            byte[] tutto = new byte[risposta.length + 2];
            tutto[0] = (byte) (200 >> 8);
            tutto[1] = (byte) 200;
            System.arraycopy(risposta, 0, tutto, 2, risposta.length);
            return tutto;
        } catch (IOException | RuntimeException e) {
            return statoRete(0);
        } finally {
            if (orologio != null) orologio.cancel();
            if (c != null) c.disconnect();
        }
    }

    private static final long DURATA_MASSIMA_RETE = 25000;

    /**
     * Il cancello della rete: chiuso all'avvio, lo apre Rust solo per un timestamp a cui chi usa
     * l'app ha appena dato il consenso, e lo richiude alla fine (roadmap della sicurezza, livello 1).
     */
    private volatile boolean reteConsentita = false;

    public void tascabileConsentiRete(boolean aperta) {
        reteConsentita = aperta;
    }

    /**
     * I server del timestamp, gli unici a cui l'app parla: aggregatori ed esploratori (AGGREGATORI
     * ed ESPLORATORI in crates/prova-data/src/server.rs) e i calendari fidati (SERVER_FIDATI in
     * formato.rs). Un secondo muro dopo quello del Rust (revisione del 31/10, punto 7): una prova
     * controlla che i due elenchi restino uguali.
     */
    private static final java.util.Set<String> SERVER_AMMESSI = new java.util.HashSet<>(java.util.Arrays.asList(
            "a.pool.opentimestamps.org", "b.pool.opentimestamps.org", "a.pool.eternitywall.com", "ots.btc.catallaxy.com",
            "blockstream.info", "mempool.space"));
    private static final String[] CALENDARI_AMMESSI = {".calendar.opentimestamps.org", ".calendar.eternitywall.com", ".calendar.catallaxy.com"};

    /** Solo https, senza utente né porta, verso un server del timestamp. */
    private static boolean ammesso(URL url) {
        if (!"https".equals(url.getProtocol()) || url.getUserInfo() != null || url.getPort() != -1) return false;
        String server = url.getHost().toLowerCase(java.util.Locale.ROOT);
        if (SERVER_AMMESSI.contains(server)) return true;
        for (String calendario : CALENDARI_AMMESSI) {
            if (server.length() > calendario.length() && server.endsWith(calendario)) return true;
        }
        return false;
    }

    private static byte[] statoRete(int stato) {
        return new byte[] {(byte) (stato >> 8), (byte) stato};
    }

    /**
     * Durante la presentazione il volume giù diventa Pagina giù (avanti) e il volume su Pagina su
     * (indietro), con tutto il resto dell'evento com'è: GameActivity li passa a Rust come i tasti
     * di un telecomando. Fuori dalla presentazione il volume fa il volume.
     */
    @Override
    public boolean dispatchKeyEvent(KeyEvent e) {
        int codice = e.getKeyCode();
        if (presentazione && (codice == KeyEvent.KEYCODE_VOLUME_DOWN || codice == KeyEvent.KEYCODE_VOLUME_UP)) {
            int altro = codice == KeyEvent.KEYCODE_VOLUME_DOWN ? KeyEvent.KEYCODE_PAGE_DOWN : KeyEvent.KEYCODE_PAGE_UP;
            KeyEvent pagina = new KeyEvent(e.getDownTime(), e.getEventTime(), e.getAction(), altro, e.getRepeatCount(),
                    e.getMetaState(), e.getDeviceId(), e.getScanCode(), e.getFlags(), e.getSource());
            return super.dispatchKeyEvent(pagina);
        }
        return super.dispatchKeyEvent(e);
    }

    @Override
    protected void onDestroy() {
        // GameActivity.onDestroy aspetta che android_main finisca: con winit 0.30 non succede
        // mai (ANR), e un nuovo android_main nello stesso processo fallirebbe (RecreationAttempt).
        // Si chiude il processo: al prossimo avvio Android ne crea uno nuovo e pulito.
        android.os.Process.killProcess(android.os.Process.myPid());
        super.onDestroy();
    }
}
