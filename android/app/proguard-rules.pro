# Regole di R8 per la build di rilascio (app/build.gradle.kts), oltre a quelle di
# proguard-android-optimize.txt e a quelle che portano le librerie (l'AAR di GameActivity tiene
# Insets e WindowInsetsCompat, che il suo C++ legge per nome).
#
# Il codice nativo cerca per nome classi, metodi e campi: se R8 li toglie o li rinomina l'app si
# chiude. build.sh controlla nel dex ogni nome di android/nomi-jni.txt.

# MainActivity: i metodi nativi scritti in Rust e quelli che Rust chiama con JNI (tascabileCopy,
# tascabilePaste, tascabileSetIme...), che dal Java non chiama nessuno e R8 toglierebbe.
-keep class it.tascabile.app.MainActivity { *; }

# GameActivity e GameTextInput: il C++ di GameActivity registra i loro metodi nativi e chiama
# setWindowFlags, getWindowInsets, setState, restartInput, il costruttore e i campi di State.
# Le classi sono annotate @Keep, ma la regola scritta qui non dipende dalle regole di androidx.annotation.
-keep class com.google.androidgamesdk.** { *; }

# Privacy: GameTextInput (InputConnection, tag gti.InputConnection) scrive con Log.d il testo
# battuto a ogni tasto, senza condizioni. Le chiamate a Log.d e Log.v si tolgono; restano Log.i,
# Log.w e Log.e, che nel codice di GameActivity 4.4.0 non hanno il testo.
-assumenosideeffects class android.util.Log {
    public static int d(...);
    public static int v(...);
}
