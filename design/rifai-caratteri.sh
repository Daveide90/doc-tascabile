#!/usr/bin/env bash
# Rifà i caratteri e le icone di crates/touch/fonts dai file originali in design/font (fuori da git:
# si riscaricano con gh api da google/fonts e google/material-design-icons, vedi docs/STATO o la memoria).
# Per aggiungere un'icona: scrivere nome e codice in design/icone-scelte.txt (codici in
# MaterialSymbolsRounded.codepoints), rilanciare questo script e rigenerare crates/touch/src/icone_tabella.rs.
set -euo pipefail
cd "$(dirname "$0")/font"
OUT=../../crates/touch/fonts
python3 -m fontTools.varLib.instancer -q Fraunces.ttf wght=600 opsz=48 SOFT=50 WONK=0 -o $OUT/Fraunces-SemiBold.ttf
python3 -m fontTools.varLib.instancer -q AtkinsonHyperlegibleNext.ttf wght=400 -o $OUT/AtkinsonHyperlegibleNext-Regular.ttf
python3 -m fontTools.varLib.instancer -q AtkinsonHyperlegibleNext.ttf wght=700 -o $OUT/AtkinsonHyperlegibleNext-Bold.ttf
UNI=$(awk '{printf "U+%s,", $2}' ../icone-scelte.txt | sed 's/,$//')
for F in 0 1; do
  python3 -m fontTools.varLib.instancer -q MaterialSymbolsRounded.ttf FILL=$F wght=400 GRAD=0 opsz=24 -o ms-$F.ttf
  NOME=$([ $F = 0 ] && echo Vuote || echo Piene)
  python3 -m fontTools.subset ms-$F.ttf --unicodes="$UNI" --no-layout-closure --layout-features='' --output-file=$OUT/MaterialSymbolsRounded-$NOME.ttf
done
