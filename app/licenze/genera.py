#!/usr/bin/env python3
"""Scrive librerie-rust.txt: autori, licenze e avvisi di ogni crate Rust che entra nell'APK.

MIT, BSD, ISC, Unicode e Apache chiedono che l'avviso di copyright e il testo della licenza
accompagnino anche il programma compilato, non solo il codice sorgente. Questo script prende
l'elenco esatto delle crate compilate per Android (lo stesso comando di android/build.sh:
arm64, pacchetto tascabile, feature predefinite) e per ognuna riporta:

- nome, versione, licenza dichiarata e righe di copyright trovate nei suoi file di licenza;
- i testi MIT e Apache 2.0 una volta sola, perché sono uguali per tutte;
- il file di licenza intero quando la crate non offre MIT o Apache (BSD, ISC, Unicode, Zlib,
  CDLA, OFL e simili), una volta per testo uguale;
- i file NOTICE (Apache 2.0, sezione 4 d) delle crate e dei motori presi da git.

Si rilancia dopo ogni cambio di Cargo.lock:  python3 -I app/licenze/genera.py
Con --controlla non scrive niente e fallisce se il file non corrisponde al lock: android/build.sh
lo usa prima di ogni APK di rilascio.
"""

import hashlib
import json
import os
import re
import subprocess
import sys

RADICE = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
USCITA = os.path.join(RADICE, "app", "licenze", "librerie-rust.txt")
BERSAGLIO = "aarch64-linux-android"

# Licenze che non chiedono altro testo oltre a quelli comuni, quando sono un'alternativa (OR).
PERMESSIVE_COMUNI = {"MIT", "Apache-2.0"}
NOMI_LICENZA = re.compile(r"^(licen[cs]e|copying|copyright|unlicense)", re.I)
NOMI_NOTICE = re.compile(r"^notice", re.I)
RIGA_COPYRIGHT = re.compile(r"^\s*(?:copyright\b|\(c\)|©)", re.I)
MODELLO = re.compile(r"\[yyyy\]|\{yyyy\}|<year>|\[name of copyright owner\]|copyright notice|copyright owner|"
                     r"copyright license|copyright holders and contributors|copyright holder nor|"
                     r"copyright \(c\) <|copyright statement|copyright and license|^\s*copyright\s*$|^\(c\) you must", re.I)
EMAIL = re.compile(r"\s*<[^>]*@[^>]*>")


def esegui(*comando):
    return subprocess.run(comando, cwd=RADICE, check=True, capture_output=True, text=True).stdout


def crate_android():
    """Le crate dell'albero normale (senza build e dev) del pacchetto tascabile per arm64 Android."""
    albero = esegui("cargo", "tree", "-p", "tascabile", "--target", BERSAGLIO, "-e", "normal",
                    "--prefix", "none", "--locked", "--offline")
    voci = set()
    for riga in albero.splitlines():
        parti = riga.split()
        if len(parti) >= 2 and parti[1].startswith("v"):
            voci.add((parti[0], parti[1][1:]))
    return voci


def radice_git(cartella):
    """Per le crate prese da git: la radice del repository, dove stanno LICENSE e NOTICE."""
    parti = cartella.split(os.sep)
    if "checkouts" in parti:
        i = parti.index("checkouts")
        return os.sep.join(parti[: i + 3])
    return None


def file_che(cartella, schema):
    if not cartella or not os.path.isdir(cartella):
        return []
    return sorted(os.path.join(cartella, f) for f in os.listdir(cartella)
                  if schema.match(f) and os.path.isfile(os.path.join(cartella, f)))


def licenze_dei_caratteri(cartella):
    """I testi accanto ai file di carattere nelle sottocartelle (OFL.txt, UFL.txt, Hack-Regular.txt)."""
    trovati = []
    for sotto in sorted(os.listdir(cartella)) if os.path.isdir(cartella) else []:
        s = os.path.join(cartella, sotto)
        if os.path.isdir(s) and any(f.lower().endswith((".ttf", ".otf")) for f in os.listdir(s)):
            trovati += sorted(os.path.join(s, f) for f in os.listdir(s) if f.lower().endswith(".txt"))
    return trovati


def solo_testo_comune(testo):
    """Vero se il file è la sola Apache 2.0 o la sola MIT: i testi comuni stanno già nell'elenco."""
    altre = ("Permission to use, copy", "Redistribution and use", "ISC License", "SPDX-License-Identifier")
    if any(a in testo for a in altre):
        return False
    apache = testo.lstrip().startswith("Apache License") and len(testo) < 12000
    mit = "Permission is hereby granted, free of charge" in testo and len(testo) < 1400
    return apache or mit


def leggi(percorso):
    with open(percorso, encoding="utf-8", errors="replace") as f:
        return f.read().replace("\r\n", "\n").strip()


def copyright_di(testi):
    righe = []
    for testo in testi:
        for riga in testo.splitlines():
            riga = riga.strip().strip("*#/ ").strip()
            if RIGA_COPYRIGHT.match(riga) and not MODELLO.search(riga) and len(riga) < 200:
                riga = EMAIL.sub("", riga).rstrip(".")
                if riga not in righe:
                    righe.append(riga)
    return righe[:6]


def alternative(espressione):
    """Le alternative di primo livello di un'espressione SPDX (le parti unite da OR)."""
    e = espressione.replace("/", " OR ").strip()
    if e.startswith("(") and e.endswith(")") and e.count("(") == 1:
        e = e[1:-1]
    if " AND " in e:
        return [e]
    return [a.strip() for a in e.split(" OR ")]


def main():
    dati = json.loads(esegui("cargo", "metadata", "--format-version", "1", "--locked", "--offline"))
    nostre = set(dati["workspace_members"])
    usate = crate_android()
    pacchetti = sorted((p for p in dati["packages"] if (p["name"], p["version"]) in usate and p["id"] not in nostre),
                       key=lambda p: (p["name"].lower(), p["version"]))

    righe_crate, testi_interi, notice = [], {}, {}
    for p in pacchetti:
        cartella = os.path.dirname(p["manifest_path"])
        git = radice_git(cartella)
        licenze = file_che(cartella, NOMI_LICENZA) or file_che(git, NOMI_LICENZA)
        if p.get("license_file"):
            licenze.append(os.path.join(cartella, p["license_file"]))
        licenze += licenze_dei_caratteri(cartella)
        testi = [leggi(f) for f in licenze]
        avvisi = file_che(cartella, NOMI_NOTICE) + file_che(git, NOMI_NOTICE)
        espressione = p.get("license") or "vedi il testo della licenza"
        chi = copyright_di(testi + [leggi(f) for f in avvisi])
        if not chi:
            autori = [EMAIL.sub("", a).strip() for a in p.get("authors") or []]
            chi = ["Autori: " + ", ".join(autori)] if autori else ["Gli autori di " + p["name"]]
        righe_crate.append(f"{p['name']} {p['version']} · {espressione}\n  " + "\n  ".join(chi))

        # Basta il testo comune se fra le alternative c'è MIT o Apache 2.0.
        if not (set(alternative(espressione)) & PERMESSIVE_COMUNI):
            for t in testi:
                if solo_testo_comune(t):
                    continue
                chiave = hashlib.sha256(t.encode()).hexdigest()
                testi_interi.setdefault(chiave, [t, []])[1].append(f"{p['name']} {p['version']}")
        for f in avvisi:
            t = leggi(f)
            chiave = hashlib.sha256(t.encode()).hexdigest()
            notice.setdefault(chiave, [t, []])
            nome = f"{p['name']} {p['version']}"
            if nome not in notice[chiave][1]:
                notice[chiave][1].append(nome)

    mit = leggi(os.path.join(RADICE, "vendor", "roxmltree", "LICENSE-MIT"))
    mit = mit[mit.index("Permission is hereby granted"):]
    apache = leggi(os.path.join(RADICE, "vendor", "roxmltree", "LICENSE-APACHE"))
    riga = "\n\n" + "=" * 60 + "\n"
    parti = [
        "Le librerie Rust dentro Doc Tascabile\n\n"
        f"Le {len(pacchetti)} crate compilate nell'app per Android, ognuna con la licenza dichiarata e le righe di "
        "copyright dei suoi file di licenza. Quando una crate offre più licenze a scelta (MIT OR Apache-2.0), "
        "Doc Tascabile la usa con la MIT. Seguono i testi MIT e Apache 2.0, i testi interi delle altre licenze "
        "e i file NOTICE. Elenco generato da app/licenze/genera.py.\n\n" + "\n".join(righe_crate),
        "Licenza MIT (il testo, uguale per ogni crate che la usa; il copyright è nella riga della crate)\n\n" + mit,
    ]
    for t, chi in sorted(testi_interi.values(), key=lambda v: v[1][0]):
        parti.append("Licenza di: " + ", ".join(chi) + "\n\n" + t)
    for t, chi in sorted(notice.values(), key=lambda v: v[1][0]):
        parti.append("NOTICE di: " + ", ".join(chi) + "\n\n" + t)
    parti.append("Apache License 2.0 (il testo intero)\n\n" + apache)
    testo = riga.join(parti) + "\n"
    if os.path.expanduser("~") in testo:
        sys.exit("ERRORE: nel testo c'è un percorso di questo computer")
    if "--controlla" in sys.argv:
        attuale = leggi(USCITA) + "\n" if os.path.exists(USCITA) else ""
        sys.exit(0 if attuale == testo else "librerie-rust.txt non è aggiornato: rilancia app/licenze/genera.py")
    with open(USCITA, "w", encoding="utf-8") as f:
        f.write(testo)
    print(f"{len(pacchetti)} crate, {len(testi_interi)} testi di licenza a parte, {len(notice)} NOTICE, "
          f"{len(testo) // 1024} KB in {os.path.relpath(USCITA, RADICE)}")


if __name__ == "__main__":
    main()
