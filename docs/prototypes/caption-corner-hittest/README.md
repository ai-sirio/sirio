# Prototipo usa-e-getta — geometria e hit-test dei caption button Windows

Ticket [#289](https://github.com/ai-sirio/sirio/issues/289) (l'angolo di Fitts) e
[#297](https://github.com/ai-sirio/sirio/issues/297) (DPI ≠ 100%, monitor non primario),
mappa [#281](https://github.com/ai-sirio/sirio/issues/281).
Ramo usa-e-getta: `prototype/caption-corner-hittest`. **Non va su `main`.**

Un comando, con `sirio.exe` già avviato e una sola istanza:

```powershell
./prototype-caption-corner-hittest.ps1            # monitor 0
./prototype-caption-corner-hittest.ps1 -Monitor 1 # secondo monitor
```

Sposta la finestra sul monitor scelto, la massimizza, e per ogni punto notevole sposta il
cursore, interroga `WM_NCHITTEST` e campiona il pixel sotto il cursore. Due strumenti
indipendenti che devono concordare: se divergono, è il metodo a essere rotto, non l'app.

## Le tre trappole, tutte capaci di dare numeri plausibili e sbagliati

1. **`Get-Process sirio` può restituire più istanze**, una senza finestra. Filtrare su
   `MainWindowHandle -ne 0`.
2. **L'hit-test di gpui ignora le coordinate del messaggio.** In
   `gpui_windows/src/events.rs:947`:
   ```rust
   let area = callback();          // nessun argomento: nessuna coordinata
   ```
   Il callback chiede «quale controllo è sotto il mouse **adesso**» e ignora l'`lParam` per
   quella decisione (l'`lParam` serve più sotto, solo per la cornice di ridimensionamento).
   Sondare con `SendMessage` passando le coordinate legge `HTCAPTION` ovunque e sembra un
   fallimento del pulsante. Va spostato il cursore sul punto e atteso **~700ms**; a 260ms si
   legge ancora il punto precedente. E dopo un ciclo ripristina/sposta/massimizza serve un
   **giro di riscaldamento** da ~1.5s, o il primo sondaggio dà un falso `HTCLIENT`.
3. **Un processo DPI-unaware legge coordinate virtualizzate.** Un 4K a 175% viene riportato
   come `2194x1234` invece di `3840x2160`, e ogni conto sui pixel sbaglia di 1.75× senza che
   nulla protesti. `SetProcessDpiAwarenessContext(-4)` va chiamato **per primo**.

## Misure (Windows 11 build 26200, build di `main` a `344da5bd`)

### Monitor 0 — primario, 3440×1440, DPI 96 (100%)

Finestra massimizzata `[-8,-8 .. 3448,1400]`, sbordo 8px per lato. Pulsante 36px fisici.
Chiudi `[3412, 3448)`, di cui visibile `[3412, 3439]`.

| punto | x,y | ottenuto | pixel |
|---|---|---|---|
| bordo destro, alto (**angolo di Fitts**) | 3439,0 | `HTCLOSE` | `#C42B1C` |
| bordo destro, metà barra | 3439,11 | `HTCLOSE` | `#C42B1C` |
| chiudi, centro visibile | 3430,11 | `HTCLOSE` | `#C42B1C` |
| chiudi, primo pixel | 3412,11 | `HTCLOSE` | `#C42B1C` |
| massimizza, centro | 3394,11 | `HTMAXBUTTON` | `#2F3133` |
| minimizza, centro | 3358,11 | `HTMINBUTTON` | `#2F3133` |
| barra, fuori dai pulsanti | 3268,11 | `HTCAPTION` | `#222427` |

**Barriera del cursore sul bordo destro: sì.** L'angolo di Fitts è disponibile e colpisce
chiudi. `corner-hover-mon0.png`.

### Monitor 1 — secondario, 3840×2160, DPI 168 (175%), a sinistra del primario

Finestra massimizzata `[-3852,-12 .. 12,2088]`. **Ogni metrica scala correttamente:**

| | 100% | 175% | atteso |
|---|---|---|---|
| pulsante | 36px | **63px** | 36 × 1.75 = 63 ✓ |
| metà barra | +19px | **+21px** dal bordo finestra | — |
| sbordo | 8px | **12px** | metrica di cornice di Windows, quantizzata (non 14) |

| punto | x,y | ottenuto | pixel |
|---|---|---|---|
| bordo destro, alto | -1,0 | `HTCLOSE` | `#C42B1C` |
| bordo destro, metà barra | -1,21 | `HTCLOSE` | `#C42B1C` |
| chiudi, centro visibile | -19,21 | `HTCLOSE` | `#C42B1C` |
| chiudi, primo pixel | -51,21 | `HTCLOSE` | `#C42B1C` |
| massimizza, centro | -82,21 | `HTMAXBUTTON` | `#2F3133` |
| minimizza, centro | -145,21 | `HTMINBUTTON` | `#2F3133` |
| barra, fuori dai pulsanti | -303,21 | `HTCAPTION` | `#222427` |

**Barriera del cursore sul bordo destro: no.** Il bordo destro del monitor 1 è `x=0`, in
comune col bordo sinistro del primario: il cursore non si ferma, prosegue sull'altro schermo.
L'angolo di Fitts **non esiste** lì — proprietà della disposizione dei monitor, identica per
qualunque app nativa, non del nostro codice. L'hit-test resta comunque corretto.

Glifi nitidi e centrati a 175%, glifo di *ripristina* corretto per una finestra massimizzata
(`corner-hover-mon1.png`).

## Verdetto

**Le metriche fisse reggono.** I 36 × 38px sono logici e gpui li scala: 63px fisici a 175%,
hit-test e hover corretti su ogni punto, su entrambi i monitor, con i due strumenti sempre
concordi. Nessuna compensazione necessaria da nessuna parte.
