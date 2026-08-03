# Sidebar a tre livelli — design

Continuazione del sistema di design (`2026-08-03-electron-design-system-design.md`,
sezione «La sidebar: tre livelli, e un pannello in meno»). Questo documento fissa
le decisioni rimaste aperte e delimita il lavoro.

## Cosa esiste già, e non si ricostruisce

- La gerarchia V1 progetto → worktree → tab è viva: `buildTree` (`tree-model.ts`)
  produce nodi a profondità 0/1/2, `SidebarTree`/`TreeRow` li rendono.
- `AgentStatus` (`running | needs-input | done | error`) è condiviso, e
  `rollupStatus` (`status-rollup.ts`) fa già salire lo stato più urgente sulle
  righe che raggruppano, con la precedenza giusta (`needs-input` batte `error`
  batte `running`).
- `StatusDot.svelte` rende già lo stato su ogni riga, con **forma oltre al
  colore** (cerchio pieno, cerchio vuoto pulsante, triangolo, trattino) e
  `prefers-reduced-motion` rispettato. I colori di stato sono alias della
  palette git (`--t-git-staged`, `--t-git-modified`, `--t-git-conflict`,
  `--t-rail-tool`): **si tengono così** — niente token `--t-status-*` nuovi
  finché due usi non divergono davvero.
- `AgentsPanel.svelte` **esiste** anche in Electron (montato in `App.svelte`,
  righe da `agentRows`, click → `revealAgent`) — la prima stesura di questa
  spec diceva il contrario, smentita leggendo il codice. Va **rimosso davvero**,
  e ha una proprietà che l'albero oggi non ha e che va assorbita prima di
  toglierlo: vede i pane agente di **tutti** i worktree, mentre `tabRows`
  popola solo il worktree attivo. Il pannello destro resta Files/Changes.

## Le quattro cose che mancano

### 0. Lo stato di TUTTI i worktree nell'albero (prerequisito della rimozione)

`tabPerWorktree` si costruisce solo per il worktree attivo; per gli altri il
rollup è `null` e pallini/contatore resterebbero muti proprio dove servono.
Si generalizza la sorgente: per ogni worktree non attivo, le righe tab si
derivano da `model.panes` (che è già globale — è la stessa fonte di
`agentRows`). Solo allora `AgentsPanel` si rimuove: componente, `agentRows`,
`revealAgent` come callback del pannello (il reveal resta, agganciato al click
sulla riga dell'albero), e la colonna da 220px in `App.svelte`. Il criterio
e2e `fase-4b-viste.spec.ts:328` che oggi punta `data-agents-worktree` migra
sulla proprietà equivalente dell'albero — la proprietà (un agente altrove è
visibile e raggiungibile) si conserva, cambia il posto dove si misura.

### 1. L'identità dell'agente sulla riga del tab

`TabRow` oggi porta `id`, `title`, `status`; l'`agentId` esiste nei pane di
`AppModel` ma non arriva all'albero. Si aggiunge `agentId: string | null` a
`TabRow` e a `TreeNode`, e sulla riga di profondità 2 compare un **monogramma**
(`AgentIcon.svelte`): una lettera per agente su un badge tondo tinto per agente.
Niente pipeline di asset ora: il monogramma è un componente condiviso che la
rifinitura visiva potrà riempire di icone vere senza toccare i call-site.

### 2. Il badge di inattività (Zzz)

Oggi nessuno registra *quando* uno stato è cambiato, quindi «inattivo da un po'»
non è derivabile. Lo store degli stati nel renderer registra `changedAt`
(epoch ms) a ogni transizione; il badge `Zzz` compare su una riga in stato
`done` da più di `IDLE_BADGE_MS` (10 minuti, costante nominata). La derivazione
è una funzione pura (`idleBadge(status, changedAt, now)`) testata a livello
unitario con un clock iniettato; **nessun criterio e2e** — aspettare dieci
minuti in Playwright non è una verifica, e un clock finto iniettato
nell'app vera non dimostrerebbe più del test unitario. Dichiarato.

### 3. Il riepilogo sull'intestazione collassata

Decisione presa (utente, 2026-08-03): **ordine stabile, opzione B** — le righe
non si riordinano mai per stato; la domanda di Briq «cosa aspetta me?» la
risponde l'intestazione. Un worktree **collassato** che contiene sessioni in
`needs-input` mostra un contatore accanto al pallino di rollup (es. `2` su
badge). Espanso, il contatore sparisce: le righe si vedono. Stessa regola sul
nodo progetto collassato.

## Interazione

Click su una riga tab = selezione del tab corrispondente (già così per V1).
Nessun menu nuovo, nessun drag nuovo: fuori scopo.

## Il rischio della larghezza — rimandato con criterio

Tre livelli in ~230px: il rientro mangia i nomi. Le vie d'uscita (guide sottili
invece di rientri larghi; worktree come intestazione) si valutano **sulla
cattura di schermata** della fase, non adesso. La cattura è parte della
consegna, non un extra.

## Verifica

Criteri e2e (`e2e/sidebar-tre-livelli.spec.ts`), tutti sul ponte
«lo stato esiste → lo vedo»:

1. Un pane agente registrato mostra il monogramma del **suo** agente sulla riga
   del tab (si asserisce l'identità — `data-agent="codex"` — non l'esistenza di
   un badge qualunque).
2. `tillerctl notify --status needs-input` su un pane di un worktree
   **collassato** fa comparire il contatore sull'intestazione; un secondo pane
   in `needs-input` lo porta a 2.
3. Espandere il worktree fa sparire il contatore e mostra il pallino sulla riga
   giusta.
4. Un pane agente in un worktree **non attivo** compare nell'albero col suo
   stato, e il click sulla sua riga porta a quel worktree e a quel pane — è la
   proprietà che oggi vive in `fase-4b-viste.spec.ts:328` sul pannello, migrata
   qui. `AgentsPanel` non esiste più nel DOM (`[aria-label="Agents"]` assente).

Unit: `idleBadge` con clock iniettato (soglia, transizione, reset su nuovo
stato); `buildTree` che propaga `agentId`.

Mutazioni: (a) `agentId` non propagato → criterio 1 rosso; (b) contatore
rimosso dal rollup → criterio 2 rosso.

Scale-guard: i componenti nuovi (`AgentIcon.svelte`, il badge contatore)
passano dai token esistenti.

## Fuori scopo

- Icone vere degli agenti (rifinitura visiva, fase successiva).
- Selezione a card, tab bar con icone, respiro Superconductor (idem).
- Riordino per stato (respinto: opzione B).
- Qualunque secondo albero per stato (respinto in spec design system).
