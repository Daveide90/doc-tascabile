package it.tascabile.app;

import android.app.Activity;
import android.content.ComponentName;
import android.content.Intent;
import android.os.Bundle;
import android.util.Log;
import android.widget.Toast;

/**
 * «Apri con» da un'altra app: riceve il documento, lo gira a MainActivity e si chiude. Ha il
 * tema dell'app (il colore carta dell'avvio) e non NoDisplay: un'attività invisibile, ad app
 * chiusa, il telefono (nubia) la creava senza mai avviare il processo (prova del 7/11 sera).
 *
 * Perché non direttamente MainActivity (segnalazione dell'autore del 7/11/2026, «se apro un pdf da
 * WhatsApp o Telegram l'app si blocca su schermo bianco»): quando l'altra app passa dal selettore
 * di Android chiedendo un risultato (nel registro del telefono, Telegram con flg=0x3000001,
 * FORWARD_RESULT), Android ignora singleTask e crea una seconda MainActivity nel task dell'altra
 * app, nello stesso processo. La seconda non disegna mai, perché eframe parte una volta sola per
 * processo, e il processo resta bloccato finché non lo si toglie dalle app recenti.
 *
 * Da qui MainActivity parte con NEW_TASK e senza risultato: Android rispetta singleTask, riusa la
 * finestra aperta (onNewIntent) o ne crea una sola. Il permesso di leggere l'URI, che l'altra app
 * ha dato a questa attività, passa a MainActivity con FLAG_GRANT_READ_URI_PERMISSION: così non se
 * ne va quando questa si chiude. Chi ha chiesto un risultato riceve RESULT_CANCELED, come prima.
 */
public class ApriConActivity extends Activity {
    /** Il passaggio a MainActivity non è riuscito: senza nomi né URI. */
    private static final String NON_APERTO = "Doc Tascabile non è riuscita ad aprire il documento. Riprova, o aprilo da dentro l'app.";

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        // Ricreata (savedInstanceState): il documento l'ha già ricevuto MainActivity.
        if (savedInstanceState == null) {
            passa(getIntent());
        }
        finish();
    }

    /** Ancora aperta quando arriva un altro documento (singleTask): anche questo va avanti. */
    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        passa(intent);
        finish();
    }

    /** Il documento a MainActivity, nel suo task, senza i flag dell'altra app. */
    private void passa(Intent arrivato) {
        if (arrivato == null) {
            return;
        }
        // La copia tiene azione, URI, tipo, extra (il nome) e ClipData; i flag dell'altra app no,
        // tranne il permesso di lettura.
        Intent avanti = new Intent(arrivato);
        avanti.setComponent(new ComponentName(this, MainActivity.class));
        avanti.setFlags(Intent.FLAG_ACTIVITY_NEW_TASK | (arrivato.getFlags() & Intent.FLAG_GRANT_READ_URI_PERMISSION));
        try {
            startActivity(avanti);
        } catch (RuntimeException e) {
            // Solo il tipo dell'errore: il suo testo può avere l'URI.
            Log.w("tascabile", "apri con: passaggio a MainActivity non riuscito (" + e.getClass().getSimpleName() + ")");
            // Chi ha toccato «Apri con» lo deve sapere (revisione del 10/11): prima questa
            // attività si chiudeva e basta. Col contesto dell'app, perché l'attività sta finendo.
            Toast.makeText(getApplicationContext(), NON_APERTO, Toast.LENGTH_LONG).show();
        }
    }
}
