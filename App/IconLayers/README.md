# Layer icona Tiller per Icon Composer

Layer generati da `Tiller/Scripts/render-icon.swift` (rigenerabili).

Per la variante layered `.icon` (specular highlights, varianti
default/dark/clear/tinted) — passo manuale, ~5 min:

1. Apri Icon Composer (in Xcode 26: Xcode → Open Developer Tool).
2. Nuovo documento macOS.
3. Trascina `background-1024.png` come layer di sfondo.
4. Trascina `spokes-1024.png` come layer in primo piano.
5. Controlla le 4 varianti (default/dark/clear/tinted) nella preview.
6. Esporta `Tiller.icon` e aggiungilo al target Tiller in project.yml
   (sources) — a quel punto l'`.icon` sostituisce l'appiconset.

Finché il passo manuale non è fatto, l'app usa l'`AppIcon.appiconset`
classico generato dagli stessi layer (già funzionante nel Dock).
