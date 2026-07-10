# Gestione permessi macOS — Design

**Data:** 2026-07-10
**Stato:** approvato

## Obiettivo

Pagina di gestione dei permessi macOS in stile Orca: mostra lo stato di ogni
permesso rilevante per Tiller, permette di triggerare il prompt di sistema una
sola volta e di aprire System Settings quando il prompt non è più disponibile.
I permessi servono soprattutto ai CLI agent (claude, codex, …) che girano nei
terminali di Tiller: ereditano il TCC envelope dell'app host.

Vincolo chiave: su macOS i permessi TCC sono ricordati dal sistema per bundle
ID. L'app non può "concederli una volta per sempre" — controlla solo quando
triggerare il prompt (mai automaticamente, mai due volte), come mostrare lo
stato e come rimediare a un denial (deep link a System Settings, perché macOS
non ri-mostra mai il prompt dopo un rifiuto).

## Permessi inclusi (6)

| Permesso | Perché serve a Tiller |
|---|---|
| Notifiche | AgentNotifier: notifica fine-lavoro degli agenti (già esistente) |
| Screen Recording | Agenti che fanno screenshot / verifica UI (`screencapture`, Playwright) |
| Accessibility | Agenti computer-use: keystroke injection, controllo finestre |
| Full Disk Access | Progetti/worktree in cartelle protette da macOS |
| Automation (Apple Events) | Script degli agenti che controllano altre app (`osascript`) |
| Local Network | Dev server e discovery sulla rete locale |

## Architettura

Tutto in `App/Permissions/` — nessun nuovo package: è I/O di sistema
AppKit/TCC, non logica di dominio.

- **`PermissionKind`** — enum con i 6 casi. Per ogni caso: nome
  visualizzato, SF Symbol, descrizione breve (cosa abilita).
- **`PermissionStatus`** — enum: `granted`, `denied`, `notRequested`,
  `checkManually`.
- **`PermissionProbe`** (protocol) — per ogni kind: `status() async ->
  PermissionStatus` e `primaryAction() async` (request / trigger prompt /
  open settings). Implementazione concreta `SystemPermissionProbe` con le
  chiamate TCC reali; mock nei test.
- **`PermissionsModel`** (@Observable, @MainActor) — carica lo stato di
  tutti i permessi, espone `refresh()` e l'azione per-riga. Decide quale
  etichetta/azione mostrare in base allo stato (es. denied → "Open
  Settings").
- **`PermissionsView`** — vista unica riusata in due contesti:
  1. sezione **Permissions** nelle Settings (accanto a General, AI
     Providers, Appearance);
  2. sheet di **onboarding al primo avvio**, con bottone "Continua" in
     fondo. Flag UserDefaults `hasSeenPermissionsOnboarding`: una volta
     chiusa, mai più mostrata automaticamente.

## Detection e azione per permesso

| Permesso | Stato via | Azione bottone |
|---|---|---|
| Notifiche | `UNUserNotificationCenter.getNotificationSettings` | Request → `requestAuthorization(.alert, .sound)` (stessa semantica di AgentNotifier); denied → Open Settings |
| Screen Recording | `CGPreflightScreenCaptureAccess()` | Request → `CGRequestScreenCaptureAccess()`; denied → Open Settings |
| Accessibility | `AXIsProcessTrusted()` | Request → `AXIsProcessTrustedWithOptions` con `kAXTrustedCheckOptionPrompt` |
| Full Disk Access | Euristica: lettura riuscita di un path protetto TCC (`~/Library/Application Support/com.apple.TCC/TCC.db`) → granted; altrimenti `checkManually` | Open Settings (deep link `x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles`) |
| Automation | `AEDeterminePermissionToAutomateTarget` verso System Events con `askUserIfNeeded=false` | Trigger Prompt → Apple Event innocuo a System Events (`askUserIfNeeded=true`) |
| Local Network | Nessuna API pubblica → sempre `checkManually` | Trigger Prompt → breve browse Bonjour via `NWBrowser`, poi cancel |

Note:
- Automation è per-target su macOS: non esiste stato globale. Si sonda
  contro System Events, il target più comune per gli script degli agenti.
- Full Disk Access non ha API né di check né di request: il canary più
  affidabile è il database TCC stesso.
- Deep link System Settings per pannello: `Privacy_Notifications`,
  `Privacy_ScreenCapture`, `Privacy_Accessibility`, `Privacy_AllFiles`,
  `Privacy_Automation`, `Privacy_LocalNetwork`.

## UI

Replica del layout Orca, con lo stile Tiller esistente (`SettingsSurface`):

- Titolo "Permessi macOS", sottotitolo: i tool nei terminali ereditano i
  permessi privacy di Tiller.
- Banner informativo in alto con bottone **Refresh** (ri-esegue tutti i
  probe).
- Sei righe, ognuna: SF Symbol, nome, badge stato colorato, descrizione,
  bottone azione a destra.
- Badge: verde `GRANTED`, rosso `DENIED`, grigio `NOT REQUESTED`, grigio
  `CHECK MANUALLY`.
- Bottone per-riga: `Request` (prompt disponibile), `Trigger Prompt`
  (trigger indiretto), `Open Settings` (denied o check manuale FDA).
- Refresh automatico dello stato quando l'app torna in foreground
  (l'utente rientra da System Settings).

## Semantica "una volta sola"

- Nessun prompt automatico all'avvio: i prompt partono solo da azione
  utente esplicita (bottone in onboarding o Settings).
- macOS ricorda la risposta per bundle ID: il prompt di sistema appare al
  massimo una volta. Dopo un denial il bottone della riga diventa "Open
  Settings".
- Onboarding one-shot: flag `hasSeenPermissionsOnboarding` su
  UserDefaults.
- Il flag esistente `hasRequestedNotifyAuth` di AgentNotifier resta
  invariato: `spawnAgent` continua a richiedere le notifiche
  contestualmente se l'utente non l'ha già fatto dalla pagina permessi
  (l'API di sistema è comunque idempotente).

## Test

- Unit test su `PermissionsModel` con `PermissionProbe` mock:
  - mapping stato → badge/etichetta bottone per tutti gli stati;
  - `refresh()` aggiorna tutti i permessi;
  - azione per-riga invoca il probe giusto;
  - logica onboarding (flag non settato → mostra; settato → no).
- Le chiamate TCC reali (`SystemPermissionProbe`) non sono testabili in
  CI: confinate nell'implementazione concreta, fuori dalla logica.

## Fuori scope

- Richiesta permessi per singolo CLI/agente (il TCC envelope è per bundle
  ID dell'app host, non per processo figlio).
- Microfono, Camera, USB, Bluetooth (presenti in Orca, non necessari a
  Tiller oggi).
- Monitoraggio push dei cambiamenti TCC (solo refresh manuale + foreground).
