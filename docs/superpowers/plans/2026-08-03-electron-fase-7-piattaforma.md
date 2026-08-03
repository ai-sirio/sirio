# Fase 7 — La piattaforma: piano di implementazione

> **Per chi esegue:** un task alla volta, un commit per task. Il gate completo
> (`bash scripts/ci.sh`) lo esegue **Claude**, mai chi implementa.

**Obiettivo:** chiudere la migrazione con ciò che rende Tiller un'applicazione
macOS invece di una finestra.

**Architettura:** sei pezzi indipendenti. Solo il primo tocca codice di una fase
precedente; gli altri cinque non si toccano fra loro.

**Spec:** `docs/superpowers/specs/2026-08-03-electron-fase-7-piattaforma-design.md`

## Vincoli globali

- Repo: `~/Desktop/Progetti/tiller-electron`; il repo Swift è **sola lettura**.
- **Nessuna chiave privata entra nel repository, in nessuna forma, in nessun
  task.** La pubblicazione firmata è un'operazione dell'utente sulla propria
  macchina.
- Le credenziali dei CLI si **leggono** da dove i CLI le tengono già: non si
  copiano, non si riscrivono, non finiscono in un archivio nostro. È la stessa
  regola degli adattatori degli agenti, che scrivono solo configurazione locale al
  worktree e mai quella globale dell'utente.
- Stringhe rivolte all'utente in inglese; commenti e nomi interni in italiano.
- Conventional Commits in inglese minuscolo imperativo; `npm run lint` a 0 errori.
- **Ogni colore e ogni misura passano dai token** di `assets/tokens.css`. Riguarda
  i due componenti nuovi di questa fase — `PermissionsPanel.svelte` (Task 3) e
  `UpdateToast.svelte` (Task 6): un esadecimale letterale o un `px` fuori scala li
  fa fallire in `src/renderer/src/lib/design/scale-guard.test.ts`, che gira nel
  gate.

## Regola d'ordine

**I task 2-6 non dipendono l'uno dall'altro.** Se uno si rivela più lungo del
previsto, si consegna senza: è la ragione per cui questa fase sta alla fine. Il
Task 1 invece viene per primo perché è l'unico che tocca codice già scritto.

---

## Task 1: La barra dei menu, e la finestra che si nasconde

**File:**
- Crea: `src/main/window/tray.ts`
- Modifica: `src/main/index.ts` (l'ascoltatore `close` messo dalla 6b-2)
- Test: `src/main/window/close-policy.test.ts`
- Crea: `e2e/fase-7-piattaforma.spec.ts` (criteri 1 e 2)

**Il punto delicato dell'intera fase.** La 6b-2 ha messo una guardia su
`window.on('close')` per i buffer non salvati. Nascondere la finestra invece di
chiuderla passa da **quello stesso evento**. Se i due comportamenti si scrivono
senza guardarsi si ottiene una delle due:

- una finestra che chiede «vuoi salvare?» ogni volta che la si nasconde, oppure
- una guardia che non scatta più, e i buffer sporchi evaporano.

- [ ] **Passo 1: scrivere i criteri e2e 1 e 2, e lasciarli rossi**

1. Nascondere la finestra con un buffer sporco **non** chiede di salvare, e il
   buffer sopravvive.
2. L'icona nella barra dei menu riapre la finestra nascosta, col buffer intatto.

Il criterio 1 misura entrambe le metà: che nascondere non chieda, **e** che il
buffer resti. Un criterio che verificasse solo la prima passerebbe anche con una
finestra distrutta.

- [ ] **Passo 2: rosso.**

- [ ] **Passo 3: implementare**

`Tray` più `window.hide()`. La decisione — nascondere o chiudere davvero — è una
funzione pura accanto a `closeDecision`, che già esiste, così la si prova senza
finestre.

Cicatrice dalla versione Swift, da tenere presente anche se l'API è diversa: una
finestra nascosta ha `canBecomeMain = false`, e la riapertura non funziona se non
lo si sa. In Electron il sintomo equivalente è una finestra che torna visibile ma
non prende il fuoco.

- [ ] **Passo 4: verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: hide the window to the menu bar instead of closing it"
```

---

## Task 2: Le notifiche

**File:**
- Crea: `src/main/notify.ts`
- Test: `src/main/notify.test.ts`
- Modifica: `e2e/fase-7-piattaforma.spec.ts` (criterio 3)

- [ ] **Passo 1: scrivere il criterio 3, e lasciarlo rosso**

3. Un agente che finisce mentre la finestra **non** è a fuoco produce una
   notifica; con la finestra a fuoco, no.

- [ ] **Passo 2: rosso.**

- [ ] **Passo 3: implementare**

Si notifica quando un agente **finisce** o **chiede qualcosa**, e solo a finestra
non a fuoco. La condizione «non a fuoco» **è** la funzionalità: notificare un
evento che l'utente sta già guardando è rumore, e un'applicazione che fa rumore
viene silenziata per sempre al terzo giorno.

La decisione «notificare sì o no» è una funzione pura di (genere di evento, fuoco,
permesso concesso): si prova senza far comparire niente sullo schermo.

Il permesso di notifica si chiede **quando serve la prima notifica**, non
all'avvio: all'avvio non significa niente per l'utente, e una richiesta senza
contesto si nega per riflesso.

- [ ] **Passo 4: verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: notify when an agent finishes out of focus"
```

---

## Task 3: I permessi di sistema

**File:**
- Crea: `src/main/permissions/probe.ts`, `src/renderer/src/lib/settings/PermissionsPanel.svelte`
- Test: `src/main/permissions/probe.test.ts`
- Modifica: `e2e/fase-7-piattaforma.spec.ts` (criterio 4)

- [ ] **Passo 1: scrivere il criterio 4, e lasciarlo rosso**

4. Il pannello mostra **«unknown»** dove non c'è una sonda, e offre il
   collegamento alle Impostazioni di Sistema.

- [ ] **Passo 2: rosso.**

- [ ] **Passo 3: implementare**

**La parità qui è parziale, ed è la cosa importante di questo task.** La sonda
Swift interroga TCC; Electron espone `systemPreferences` per alcune categorie e
non per altre. Dove non c'è una sonda lo stato è **sconosciuto**, e si offre il
collegamento alle Impostazioni.

**Non dedurre un permesso da un tentativo riuscito.** Il costo dell'errore è
asimmetrico: se la deduzione sbaglia, l'interfaccia dichiara concesso qualcosa che
non lo è, e l'utente cerca il guasto ovunque tranne lì. Uno stato «sconosciuto»
non è una lacuna del pannello: è l'unica cosa vera che si può dire.

Lo stato è un'unione di tre valori — `granted`, `denied`, `unknown` — non un
booleano. Un booleano costringe a scegliere fra due bugie.

- [ ] **Passo 4: verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: report system permissions without guessing"
```

---

## Task 4: Il consumo delle quote

**File:**
- Crea: `src/main/usage/` — un recuperatore per provider
- Test: `src/main/usage/*.test.ts`
- Modifica: `e2e/fase-7-piattaforma.spec.ts` (criterio 5)

- [ ] **Passo 1: scrivere il criterio 5, e lasciarlo rosso**

5. Il consumo appare per un provider configurato, e la sua assenza **non rompe
   nulla** per gli altri.

- [ ] **Passo 2: rosso.**

- [ ] **Passo 3: implementare**

Tre recuperatori: Ollama Cloud, OpenCode Go, e Codex via **OAuth diretto** — nella
versione finale, non in quella storica. In Swift Codex è nato come sottoprocesso
dell'app-server ed è stato sostituito da una chiamata HTTP con OAuth sul modello
di codexbar: si porta la seconda.

Ogni recuperatore è indipendente e il suo fallimento è **locale**. Un provider non
configurato è uno stato normale, non un errore: nessun `catch` vuoto, ma nemmeno
un errore che si propaga e spegne il pannello per tutti.

- [ ] **Passo 4: verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: show quota usage per configured provider"
```

---

## Task 5: Registro e installazione degli agenti

**File:**
- Crea: `src/main/agents/registry.ts`, `src/main/agents/installer.ts`
- Test: i rispettivi
- Modifica: `e2e/fase-7-piattaforma.spec.ts` (criterio 6)

- [ ] **Passo 1: scrivere il criterio 6, e lasciarlo rosso**

6. Un manifest assente **non impedisce** alla fabbrica di costruire i driver
   nativi.

È il criterio che protegge la scelta della 5a: il registro serve solo agli agenti
ACP installabili, e con i quattro nativi presenti non deve bloccare nulla.

- [ ] **Passo 2: rosso.**

- [ ] **Passo 3: implementare**

**Installare software è l'operazione più delicata dell'intera migrazione.** Il
codice deve dire da dove viene ciò che si installa, come si verifica, e cosa
succede se la verifica fallisce. Un installatore che scarica ed esegue senza
controllo è una vulnerabilità con un'interfaccia grafica.

Se la verifica non è implementabile con quello che il registro espone, **non si
installa**: si mostra il comando che l'utente può eseguire da sé. Un'installazione
guidata e verificabile è meglio di una automatica e cieca, e in caso di dubbio la
scelta è dell'utente — che è anche la regola che il progetto applica già altrove,
non toccando mai la configurazione globale.

- [ ] **Passo 4: verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: read agent manifests and install on demand"
```

---

## Task 6: L'aggiornamento automatico

**File:**
- Modifica: `package.json`, la configurazione di pubblicazione
- Crea: `src/main/updater.ts`, `src/renderer/src/lib/UpdateToast.svelte`
- Test: `src/main/updater.test.ts`

- [ ] **Passo 1: scrivere i test che falliscono**

Che il controllo parta all'avvio e a intervalli; che una versione nuova produca
l'avviso; che un errore di rete **non** disturbi l'utente.

- [ ] **Passo 2: rosso.** - [ ] **Passo 3: implementare.**

L'avviso è un toast proprio, non il pannello del framework: è la scelta già fatta
nella versione Swift e si porta.

**Sulla firma, nessuna ambiguità.** Sparkle firma con EdDSA e tiene la chiave nel
portachiavi; `electron-updater` verifica la firma del codice del bundle. Sono
meccanismi diversi con custodie diverse: il piano si ferma alla configurazione e
al canale, e il rilascio vero resta fuori.

- [ ] **Passo 4: verde.** - [ ] **Passo 5: committare**

```bash
git commit -m "feat: check for updates and offer them quietly"
```

**Non c'è un criterio end-to-end per questo task, ed è dichiarato:** provarlo
richiede di pubblicare una versione firmata. Si verifica che la configurazione sia
quella attesa e che il controllo parta; il resto è verifica manuale al primo
rilascio vero, come è stato per Sparkle. Riportarlo come coperto sarebbe falso.

---

## Chiusura

Riportare: i commit; `npx vitest run src/main/`; `npm run typecheck` e
`npm run lint` (0 errori); quali criteri sono stati saltati e perché; ogni punto
del piano trovato sbagliato; e ciò che non si è riusciti a verificare.

Fuori scope: il rilascio vero e la custodia delle chiavi, che sono operazioni
dell'utente; e qualunque funzionalità nuova rispetto alla versione Swift.
