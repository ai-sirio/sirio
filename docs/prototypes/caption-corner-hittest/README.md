# Prototipo usa-e-getta — l'angolo dello schermo colpisce il pulsante chiudi?

Ticket [#289](https://github.com/ai-sirio/sirio/issues/289), mappa [#281](https://github.com/ai-sirio/sirio/issues/281).
Ramo usa-e-getta: `prototype/caption-corner-hittest`. **Non va su `main`.**

Un comando:

```powershell
# con sirio.exe gia' avviato, una sola istanza
./prototype-caption-corner-hittest.ps1
```

## La domanda

Massimizzata, una finestra Windows sborda di 8px oltre ogni bordo dello schermo. I
caption button sono a filo del bordo **della finestra**, quindi gli 8px piu' a destra
del pulsante chiudi cadono **fuori dallo schermo**. Puntare l'angolo esatto dello
schermo — il caso di Fitts, che e' il motivo per cui i pulsanti sono a filo — colpisce
comunque il pulsante chiudi?

## La trappola che ha reso sbagliata la prima versione

Sondare `WM_NCHITTEST` con `SendMessage` passando le coordinate nell'`lParam` **non
misura il punto richiesto**. In `gpui_windows/src/events.rs:947`:

```rust
let callback = self.state.callbacks.hit_test_window_control.take();
let area = callback();          // nessun argomento: nessuna coordinata
```

Il callback chiede «quale controllo e' sotto il mouse **adesso**» e ignora l'`lParam`
per quella decisione (l'`lParam` serve piu' sotto, solo per la cornice di
ridimensionamento). Un sondaggio sintetico risponde quindi per la posizione **reale**
del cursore. La v1 leggeva `HTCAPTION` ovunque perche' il cursore stava fermo sulla
zona di trascinamento.

Correzione: spostare davvero il cursore sul punto, lasciar girare il message loop,
**poi** sondare. Serve anche un assestamento generoso: a 260ms il sondaggio legge
ancora il punto precedente, a 700ms e' stabile.

## Misura (build 26200, monitor 3440x1440, DPI 96 / 100%)

Finestra massimizzata `3456x1408 @ -8,-8`; sbordo 8px per lato. Pulsante chiudi
`[3412, 3448)` in coordinate schermo, di cui visibile `[3412, 3439]`.

| punto | x,y | atteso | ottenuto | pixel chiudi |
|---|---|---|---|---|
| ANGOLO schermo (caso di Fitts) | 3439,0 | HTCLOSE | **HTCLOSE** | `#C42B1C` |
| bordo destro, meta' barra | 3439,11 | HTCLOSE | **HTCLOSE** | `#C42B1C` |
| chiudi, centro visibile | 3426,11 | HTCLOSE | **HTCLOSE** | `#C42B1C` |
| chiudi, primo pixel | 3412,11 | HTCLOSE | **HTCLOSE** | `#C42B1C` |
| chiudi, riga 0 dello schermo | 3426,0 | HTCLOSE | **HTCLOSE** | `#C42B1C` |
| oltre il bordo (cursore bloccato a 3439) | 3446,11 | HTCLOSE | **HTCLOSE** | `#C42B1C` |
| massimizza, centro | 3394,11 | HTMAXBUTTON | **HTMAXBUTTON** | — |
| minimizza, centro | 3358,11 | HTMINBUTTON | **HTMINBUTTON** | — |
| barra, fuori dai pulsanti | 3280,11 | HTCAPTION | **HTCAPTION** | — |

`corner-hover.png` e' il ritaglio dell'angolo col cursore parcheggiato su `(3439, 0)`:
chiudi acceso di rosso pieno, × bianca, glifo di massimizza correttamente quello di
*ripristina*.

## Verdetto

**Si', l'angolo colpisce.** I due strumenti indipendenti — il codice `HT*` restituito
dall'hit-test e il pixel dell'hover — concordano su ogni punto. Lo sbordo di 8px
consuma 8 dei 36px del pulsante chiudi, ma il cursore si ferma a `x=3439`, che cade
dentro `[3412, 3448)`: **nessuna compensazione necessaria**.
