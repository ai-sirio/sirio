# Fase 7 — La piattaforma: design

Data: 2026-08-03
Stato: proposto

## Obiettivo

Chiudere la migrazione con ciò che rende Tiller un'applicazione macOS invece di
una finestra: aggiornamento automatico, icona nella barra dei menu, notifiche,
permessi di sistema, consumo delle quote, e l'installazione degli agenti rimandata
dalla 5a.

## Perché è l'ultima

Ognuno di questi pezzi è **indipendente dagli altri** e nessuno è un prerequisito
per il lavoro precedente. Una fase fatta di pezzi scollegati è la peggiore da
mettere all'inizio — non insegna niente sull'architettura — e la migliore da
mettere alla fine, perché ogni pezzo può fallire senza bloccare i vicini.

Da questo segue una regola per il piano: **un task per pezzo, e nessuna dipendenza
fra task**. Se uno si rivela più lungo del previsto, si consegna senza.

## 1. Aggiornamento automatico

Sparkle → `electron-updater`.

La sostanza della funzionalità è già stata decisa nella versione Swift e non si
ridiscute: un controllo periodico, un avviso non invadente quando c'è una
versione nuova, l'installazione al riavvio. In Swift l'avviso è un toast proprio
invece del pannello di Sparkle, e quella scelta si porta.

**La parte che non si porta è la firma.** Sparkle firma con EdDSA e tiene la
chiave privata nel portachiavi; `electron-updater` verifica la firma del codice
del bundle. Sono meccanismi diversi con custodie diverse, e il piano deve dirlo
esplicitamente invece di lasciar credere che sia la stessa cosa con un nome
nuovo.

**Nessuna chiave privata entra nel repository, in nessuna forma, in nessun task.**
La pubblicazione firmata è un'operazione dell'utente sulla propria macchina, non
di chi implementa: il piano si ferma alla configurazione e al canale di
aggiornamento, e il rilascio vero resta fuori.

## 2. Icona nella barra dei menu e chiusura della finestra

In Swift: `MenuBarStatusIcon` (32 righe) e `HideOnCloseWindowDelegate` (59), con
una cicatrice registrata — chiudere la finestra la **nasconde** invece di
distruggerla, e una finestra nascosta ha `canBecomeMain = false`, che rompe la
riapertura se non lo si sa.

In Electron il pezzo equivalente è `Tray` più `window.hide()`, e la trappola
cambia forma ma non sparisce: **la 6b-2 ha appena messo una guardia su
`window.on('close')`** per i buffer non salvati. Nascondere la finestra invece di
chiuderla passa da quello stesso evento. Se i due comportamenti si scrivono senza
guardarsi, si ottiene una delle due: una finestra che chiede di salvare ogni volta
che la si nasconde, oppure una guardia che non scatta più.

È l'unico punto di questa fase che tocca codice di una fase precedente, e per
questo il suo task viene per primo.

## 3. Notifiche

`AgentNotifier` (118 righe) → l'API `Notification` di Electron.

La sostanza: si notifica quando un agente **finisce** o **chiede qualcosa** mentre
la finestra non è a fuoco. La condizione "non a fuoco" è la funzionalità: notificare
un evento che l'utente sta già guardando è rumore.

Le notifiche di macOS richiedono il permesso dell'utente, e la richiesta va fatta
quando serve — non all'avvio, quando non significa niente.

## 4. Permessi di sistema

`SystemPermissionProbe` (5.9K) più il pannello delle impostazioni e il foglio di
primo avvio.

**Qui la parità è parziale, e va detto invece che scoperto a metà.** La sonda
Swift interroga TCC per capire cosa è già concesso; Electron espone
`systemPreferences` per alcune categorie e non per altre. Dove non c'è una sonda,
l'unica strada onesta è **mostrare lo stato come sconosciuto e offrire il
collegamento alle Impostazioni di Sistema**, non dedurlo da un tentativo riuscito.

Dedurre un permesso da un tentativo ha un costo asimmetrico: se la deduzione
sbaglia, l'interfaccia dichiara concesso qualcosa che non lo è, e l'utente cerca
il guasto ovunque tranne che lì.

## 5. Consumo delle quote

Tre recuperatori nella versione Swift: Ollama Cloud, OpenCode Go, e Codex via
OAuth diretto — quest'ultimo con una storia utile, perché è nato come sottoprocesso
dell'app-server ed è stato **sostituito** da una chiamata HTTP con OAuth, sul
modello di codexbar. Si porta la versione finale, non quella storica.

Le credenziali si leggono da dove i CLI le tengono già. **Non si copiano, non si
riscrivono, non si mettono in un archivio nostro:** la stessa regola che vale per
gli adattatori degli agenti, che scrivono solo configurazione locale al worktree e
mai quella globale dell'utente.

## 6. Registro e installazione degli agenti

`AgentRegistryClient`, `AgentInstaller`, `AgentInstallStore`, rimandati qui dalla
5a con una motivazione precisa: servono solo agli agenti ACP installabili, e con i
quattro nativi presenti non bloccano nulla.

La fabbrica dei driver legge i manifest **se ci sono** e ignora l'assenza —
comportamento già previsto nella 5a, che questa fase riempie.

Installare software è l'operazione più delicata dell'intera migrazione. Il piano
deve dire da dove viene ciò che si installa, come si verifica, e cosa succede se
la verifica fallisce. Un installatore che scarica ed esegue senza controllo è una
vulnerabilità con un'interfaccia grafica.

## Come si verifica

Ogni pezzo ha i suoi criteri e non dipende dagli altri:

1. Nascondere la finestra **non** chiede di salvare; chiuderla con buffer sporchi
   **sì** (il pezzo 2 contro la guardia della 6b-2).
2. L'icona nella barra dei menu riapre la finestra nascosta.
3. Un agente che finisce mentre la finestra non è a fuoco produce una notifica;
   con la finestra a fuoco, no.
4. Il pannello dei permessi mostra "sconosciuto" dove non c'è una sonda, e non
   inventa.
5. Il consumo delle quote appare per un provider configurato, e la sua assenza
   non rompe nulla per gli altri.
6. Un manifest assente non impedisce alla fabbrica di costruire i driver nativi.

Sull'aggiornamento automatico non c'è un criterio end-to-end, e va detto: provarlo
richiede di pubblicare una versione firmata. Si verifica che la configurazione sia
quella attesa e che il controllo parta; il resto è verifica manuale al primo
rilascio vero, come è stato per Sparkle.

## Fuori scope

- Il rilascio vero e la custodia delle chiavi: operazioni dell'utente.
- Qualunque funzionalità nuova rispetto alla versione Swift.
