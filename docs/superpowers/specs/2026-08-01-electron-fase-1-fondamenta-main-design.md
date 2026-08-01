# Fase 1 — Fondamenta del main (design)

Data: 2026-08-01
Fase precedente: [Fase 0 — walking skeleton](2026-08-01-tiller-electron-migration-design.md)
Repo di destinazione: `~/Desktop/Progetti/tiller-electron/`

## Obiettivo

Costruire il processo main completo — persistenza, git, adapter agenti — senza
una riga di interfaccia. Tutto ciò che questa fase aggiunge si pilota da
`tillerctl` e si verifica da riga di comando.

L'ordine orizzontale deciso nel design di migrazione (prima il backend, poi la
UI) vale qui nella sua forma più pura: alla fine della Fase 1 l'app apre una
finestra vuota, ma sa già tenere progetti, creare worktree git reali e lanciare
i cinque agenti.

## Contesto misurato

Il codice Swift che questa fase sostituisce è poco:

| Package | Righe | Contenuto |
|---|---|---|
| `TillerGit` | 906 | shell-out a git, parser di output |
| `TillerAgents` | 606 | 5 adapter, quoting, provisioning skill |
| `TillerPersistence` | 578 | GRDB, 17 migrazioni, record |

Il database di produzione (`~/Library/Application Support/Tiller/tiller.sqlite`,
12,1 MB, migrazioni `v1`…`v17` applicate) contiene:

```
    10  project           9  worktree        121  paneScrollback
    62  chatSession    1000  chatItem          9  workspaceLayout
     8  workspaceTab      2  terminalContent
     0  agentAccount      0  agentSession      0  workspaceLayoutQuarantine
```

`agentAccount` e `agentSession` sono vuote in produzione: lo schema le prevede,
il codice non le popola. Vanno create per completezza dello schema, ma nessuna
funzionalità della Fase 1 dipende dal loro contenuto.

## Ricerca preliminare — riuso contro riscrittura

Requisito esplicito dell'utente: usare librerie mature dove esistono. Verifica
fatta contro il registro npm il 2026-08-01, con prove eseguite localmente e non
dedotte.

| Area | Libreria | Download/sett | Esito |
|---|---|---|---|
| Git | `simple-git` 3.36.0 | 12,2 M | **Adottata** |
| Git puro JS | `isomorphic-git` 1.40.0 | — | Scartata |
| SQL + migrazioni | `kysely` 0.29.4 | 12,9 M | **Adottata** |
| SQL (alternativa) | `drizzle-orm` 0.45.2 | 17,4 M | Scartata |
| Driver SQLite | `better-sqlite3` 13.0.2 | 9,0 M | **Adottata** |
| Dialect `node:sqlite` | `kysely-node-sqlite` 1.1.0 | 21 | Scartata |
| TOML | `smol-toml` 1.7.1 | — | **Adottata** |
| Worktree git | nessun pacchetto sopra 10 download/sett | — | Nessuna |

### Driver SQLite — decisione ribaltata rispetto al design di migrazione

Il design di Fase 0 prevedeva `node:sqlite` (stdlib di Node 24) per evitare
moduli nativi da ricompilare. Quella scelta viene **ribaltata**, perché la
premessa che la sosteneva non è più vera.

Verifiche eseguite:

- `node:sqlite` funziona sia in Node 24.18.1 sia dentro Electron 43.2.0 —
  WAL, transazioni, BLOB e `backup()` tutti presenti. La scelta era tecnicamente
  praticabile.
- `better-sqlite3` 13.0.2 **carica dentro Electron 43.2.0 senza alcun rebuild**:
  è passato a N-API (`node-addon-api`) con prebuilds per piattaforma, esattamente
  come node-pty. L'unica obiezione contro di esso era il costo di ricompilazione,
  e quel costo non esiste più.

Il driver da solo non è però la decisione: sopra di esso servono un migratore e
query tipizzate su 11 tabelle. Né `kysely` né `drizzle-orm` offrono un dialect
ufficiale per `node:sqlite`; quelli disponibili su npm fanno 21 e 4 download a
settimana e l'ultimo aggiornamento risale a oltre un anno fa. Adottarli
significherebbe sostituire codice artigianale nostro con codice artigianale
altrui, non manutenuto — il contrario del requisito.

Scegliendo `better-sqlite3` si ottiene il dialect di prima classe di kysely, il
suo `Migrator` con `FileMigrationProvider` e le query tipizzate, senza scrivere
infrastruttura.

Costo accettato: due moduli nativi invece di uno, e la necessità di dichiarare i
tipi delle tabelle in TypeScript. Il secondo è lavoro reale che restituisce
errori in compilazione invece che a runtime.

Rischio residuo: il packaging del binario nativo. È la stessa classe di bug di
`spawn-helper` in Fase 0 (binario presente ma inutilizzabile nel pacchetto), già
coperta dal pattern del test di guardia introdotto allora; va replicato per
`better-sqlite3`.

### Git — dove la libreria arriva e dove no

`simple-git` copre `status` (parsing `--porcelain` incluso), `branch`, `clone`
con handler di progresso, `diff`. Non ha **alcuna** API per i worktree: zero
occorrenze del termine nei suoi tipi pubblici.

Poiché il worktree è l'oggetto centrale di Tiller — un terminale per worktree,
una sidebar per progetto — la parte più specifica del dominio non riceve aiuto.
Ricerca su npm: nessun pacchetto dedicato ai worktree supera i 10 download
settimanali. Quel codice resta nostro (circa 40 righe sopra
`raw(['worktree', 'list', '--porcelain'])`); non è reinventare la ruota, è
constatare che non esiste.

`isomorphic-git` è scartato per lo stesso motivo, aggravato: reimplementa git in
JavaScript puro e non gestisce i worktree affatto.

## Decisioni

### D1 — I dati esistenti vengono importati

L'app Electron non condivide il database con l'app Swift. Ha il proprio, in
`~/Library/Application Support/tiller-electron/`, e un comando di import una
tantum travasa i dati da quello Swift.

Alternative scartate:

- *Ripartire puliti*: costa la riconfigurazione manuale di 10 progetti e 9
  worktree, e perde 62 sessioni di chat e 121 scrollback.
- *Condividere lo stesso file*: due processi con lock SQLite concorrenti e due
  modelli che divergono a ogni fase — si paga per anni una compatibilità che
  serve per settimane.

### D2 — Lo schema si crea intero subito

La migrazione iniziale crea tutte e 11 le tabelle nella loro forma finale, e
l'import trasferisce tutto, non solo ciò che la Fase 1 legge.

Conseguenza accettata e messa a verbale: `workspaceLayout` e `chatItem` nascono
con la forma prodotta da SwiftUI, e le Fasi 4 e 5 le rimigreranno. La
serializzazione del layout ne è la prova visibile — contiene wrapper
`{"rawValue": "…"}`, impronta del `Codable` sintetizzato di Swift su tipi
`RawRepresentable`.

Il motivo per accettarla: l'import gira **adesso**, contro un database vivo e
un'app Swift ancora installata. Frazionarlo per fasi significherebbe importare
fra mesi da una sorgente nel frattempo cambiata.

### D3 — Le 17 migrazioni Swift non vengono replicate

La storia di uno schema non è lo schema. `v13`, `v15` e `v16` sono re-keying
(`paneId` → `terminalContentID`), cioè correzioni di modello già assorbite nella
forma finale. La migrazione `001_initial` crea direttamente il risultato; le
migrazioni Electron ripartono da 1 e crescono per conto proprio.

### D4 — `prepare()` degli adapter è un no-op in Fase 1

Gli adapter Swift scrivono configurazione di hook **dentro il worktree**:

```
.claude/settings.local.json          letto, fuso, riscritto
.tiller/omp-hook.ts
.opencode/plugin/tiller-session.js
Codex                                nessun file, -c notify=[...] sulla riga di comando
```

Durante la migrazione le due app convivono e puntano quegli hook a due binari
`tillerctl` diversi. Chi apre il worktree per ultimo vince, e l'altra app smette
di ricevere notifiche **senza segnalare nulla** — il Layer B (titolo OSC)
continua a funzionare, quindi la degradazione è parziale e difficile da notare.
Questa classe di bug ha già colpito il progetto una volta (percorso `tillerctl`
stale, fix `c3cb71c` con shim a symlink).

`prepare()` esiste solo per alimentare il Layer A della detection, che è materia
di Fase 3. Scriverlo in Fase 1 produrrebbe effetti collaterali sui worktree reali
dell'utente per una funzionalità che nessuno legge ancora, al prezzo di rompere
l'app Swift — che in questi mesi resta quella in uso quotidiano.

In Fase 1 `prepare()` non scrive nulla, e un test verifica che il filesystem del
worktree resti invariato dopo la chiamata. Il test impedisce che la scelta
rientri per sbaglio.

Alternative scartate: *scrivere e vincere* (rompe l'app in uso); *coesistenza
esplicita* con chiavi e socket distinti (lavoro reale in una compatibilità
destinata a essere buttata).

### D5 — TillerGit si porta intero

A differenza dello schema, i parser git non verranno riprogettati: il loro
contratto d'ingresso è il formato di output di `git`, non una scelta di
interfaccia. Sono il codice più direttamente traducibile del repo, test inclusi.

## Struttura

```
src/main/
├── db/
│   ├── schema.ts          tipi TypeScript delle 11 tabelle (interfaccia kysely)
│   ├── database.ts        apertura, WAL, foreign_keys, percorso di supporto
│   └── migrations/
│       └── 001_initial.ts
├── import/
│   └── swift-import.ts    travaso una tantum dal database dell'app Swift
├── git/
│   ├── runner.ts          simple-git configurato: cwd, limiti di output, errori
│   ├── worktrees.ts       list/add/remove — parsing --porcelain
│   ├── repo.ts            rilevamento repo, root, branch corrente
│   ├── status.ts          .status() → modello di dominio
│   ├── branches.ts        elenco branch locali
│   ├── remote.ts          owner GitHub, nome progetto da URL di clone
│   ├── clone.ts           clone con progresso
│   └── actions.ts         stage/unstage/discard, con validazione
├── agents/
│   ├── catalog.ts         interfaccia AgentAdapter, elenco dei 5
│   ├── adapters/          claude.ts codex.ts opencode.ts pi.ts omp.ts
│   └── quote.ts           quoting shell + letterale TOML (smol-toml)
├── control/               esistente, esteso con i nuovi metodi
└── state/                 esistente, esteso con progetti e worktree
```

I confini della Fase 0 restano invariati: `control/dispatch.ts` non importa né
`electron` né `net` e resta l'unico punto di ingresso. I nuovi moduli sono
consumati dal dispatcher, non da Electron.

## Schema

Forma finale estratta dal database di produzione. Undici tabelle, con
`ON DELETE CASCADE` da `worktree` verso tutto ciò che vi appartiene.

```sql
CREATE TABLE project (
  id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL, rootPath TEXT NOT NULL,
  createdAt DATETIME NOT NULL, colorHex TEXT, displayName TEXT,
  iconKind TEXT NOT NULL DEFAULT 'icon', iconValue TEXT, avatarImage BLOB,
  defaultWorktreeBase TEXT, worktreeLocationOverride TEXT,
  orderIdx INTEGER NOT NULL DEFAULT 0);

CREATE TABLE worktree (
  id TEXT PRIMARY KEY NOT NULL,
  projectId TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
  branch TEXT NOT NULL, path TEXT NOT NULL, createdAt DATETIME NOT NULL,
  comment TEXT, commentUpdatedAt DATETIME,
  isPrimary BOOLEAN NOT NULL DEFAULT 0, orderIdx INTEGER NOT NULL DEFAULT 0);

CREATE TABLE terminalContent (
  id TEXT PRIMARY KEY NOT NULL,
  worktreeId TEXT NOT NULL REFERENCES worktree(id) ON DELETE CASCADE,
  launchKind TEXT NOT NULL, agentId TEXT, commandJSON TEXT,
  createdAt DATETIME NOT NULL);

CREATE TABLE paneScrollback (
  terminalContentId TEXT PRIMARY KEY NOT NULL,
  worktreeId TEXT NOT NULL REFERENCES worktree(id) ON DELETE CASCADE,
  data BLOB NOT NULL, updatedAt DATETIME NOT NULL);

CREATE TABLE agentAccount (
  id TEXT PRIMARY KEY NOT NULL, provider TEXT NOT NULL,
  configDirPath TEXT NOT NULL, label TEXT NOT NULL, orgName TEXT,
  createdAt DATETIME NOT NULL, lastAuthenticatedAt DATETIME NOT NULL);

CREATE TABLE agentSession (
  terminalContentId TEXT PRIMARY KEY NOT NULL,
  worktreeId TEXT NOT NULL REFERENCES worktree(id) ON DELETE CASCADE,
  agentId TEXT NOT NULL, sessionRef TEXT NOT NULL, capturedAt DATETIME NOT NULL);

CREATE TABLE chatSession (
  id TEXT PRIMARY KEY NOT NULL,
  worktreeId TEXT NOT NULL REFERENCES worktree(id) ON DELETE CASCADE,
  agentId TEXT NOT NULL, acpSessionId TEXT, createdAt DATETIME NOT NULL,
  lastActivityAt DATETIME NOT NULL, contextUsageUsed INTEGER,
  contextUsageSize INTEGER, permissionMode TEXT, selectedModel TEXT,
  selectedEffort TEXT, transportKind TEXT NOT NULL DEFAULT 'acp', title TEXT);

CREATE TABLE chatItem (
  sessionId TEXT NOT NULL REFERENCES chatSession(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL, kind TEXT NOT NULL, payload BLOB NOT NULL,
  PRIMARY KEY (sessionId, ordinal));

CREATE TABLE workspaceLayout (
  worktreeId TEXT PRIMARY KEY NOT NULL REFERENCES worktree(id) ON DELETE CASCADE,
  schemaVersion INTEGER NOT NULL, revision INTEGER NOT NULL,
  payload TEXT NOT NULL, checksum TEXT NOT NULL, updatedAt DATETIME NOT NULL);

CREATE TABLE workspaceTab (
  id TEXT PRIMARY KEY NOT NULL,
  worktreeId TEXT NOT NULL REFERENCES worktree(id) ON DELETE CASCADE,
  title TEXT NOT NULL, titleIsAutoNamed BOOLEAN NOT NULL,
  contentKind TEXT NOT NULL, contentId TEXT NOT NULL, viewStateJSON TEXT,
  viewStateVersion INTEGER NOT NULL, createdAt DATETIME NOT NULL,
  UNIQUE (worktreeId, contentKind, contentId));

CREATE TABLE workspaceLayoutQuarantine (
  id TEXT PRIMARY KEY NOT NULL,
  worktreeId TEXT NOT NULL REFERENCES worktree(id) ON DELETE CASCADE,
  payload TEXT NOT NULL, reason TEXT NOT NULL, createdAt DATETIME NOT NULL);
```

### Codifica dei campi binari

Verificata aprendo il database di produzione, non dedotta:

| Campo | Contenuto reale | Portabilità |
|---|---|---|
| `chatItem.payload` | JSON, `{"userMessage":{"blocks":[…]}}` | diretta |
| `paneScrollback.data` | byte grezzi del terminale, non compressi | diretta |
| `workspaceLayout.payload` | JSON con wrapper `{"rawValue":"UUID"}` | diretta, da svolgere in Fase 4 |
| `project.avatarImage` | dati immagine | diretta |

Le date sono `DATETIME` in forma di stringa e restano tali: nessuna conversione,
nessuna perdita di fuso orario.

## Import

Comando: `tillerctl import --from <percorso-del-db-swift>`.

Comportamento:

1. **Copia prima di leggere.** L'app Swift può essere in esecuzione; il file
   originale non viene mai aperto in scrittura né bloccato. Si copia in una
   cartella temporanea e si legge da lì in sola lettura.
2. **Una transazione sola**, in ordine di dipendenza: `project` → `worktree` →
   tutte le tabelle figlie. Un fallimento a metà non lascia un database parziale.
3. **Idempotente**: upsert per chiave primaria. Rieseguirlo non duplica, così
   può essere rilanciato quando l'app Swift avrà accumulato altro lavoro.
4. **Verifica di conteggio**: per ogni tabella, righe in sorgente contro righe in
   destinazione. Se un numero non torna l'import fallisce con un errore che dice
   quale tabella, invece di riuscire a metà.

## Superficie di controllo

Ai quattro metodi della Fase 0 (`pane.create`, `pane.write`, `state.get`,
`window.focus`) si aggiungono:

| Metodo | Parametri | Restituisce |
|---|---|---|
| `project.add` | `rootPath`, `name?` | id progetto |
| `project.list` | — | elenco progetti |
| `project.remove` | `projectId` | esito |
| `worktree.list` | `projectId?` | elenco worktree |
| `worktree.create` | `projectId`, `branch`, `base?`, `path?` | id worktree |
| `worktree.remove` | `worktreeId` | esito |
| `git.status` | `worktreeId` | snapshot di stato |
| `agent.list` | — | i 5 adapter con id e nome |
| `db.import` | `fromPath` | conteggi per tabella |

`pane.create` guadagna un parametro `agent`: quando presente, il pane esegue il
comando dell'adapter invece della shell.

Ogni metodo ha il corrispondente comando piatto in `tillerctl`, come già fa la
CLI Swift.

## Verifica

### Criteri di successo

1. **Import fedele e ripetibile.** Import da una copia del database di
   produzione: conteggi identici su tutte e 11 le tabelle. Eseguito due volte, i
   conteggi non cambiano. Cade se una riga si perde o si duplica.
2. **Worktree reale, verificato dall'esterno.** `tillerctl worktree create`
   produce un worktree che `git worktree list`, eseguito dalla shell e non dal
   nostro codice, elenca. Il record sopravvive al riavvio dell'app. L'oracolo è
   git: un test che usasse il nostro parser confermerebbe solo sé stesso.
3. **Comando agente corretto.** `tillerctl run --agent codex` avvia nel pane un
   comando il cui `-c notify=[…]` è TOML che si riparsa. Rompendo il quoting, il
   test deve tornare rosso.

### Metodo

- Test git contro repository reali creati in cartelle temporanee, non contro
  mock: `simple-git` va verificato contro git vero, non contro le nostre
  aspettative su git.
- Test di import contro una fixture sintetica costruita con lo schema Swift, più
  una verifica manuale contro il database di produzione da 12 MB.
- Test di guardia sul packaging del binario `better-sqlite3`, sul modello di
  quello introdotto in Fase 0 per `spawn-helper`.
- Test che verifica che `prepare()` non scriva nulla nel worktree.
- Il gate resta `bash scripts/ci.sh`, che deve stampare `CI OK`.

## Fuori perimetro

- Qualsiasi interfaccia: la Fase 1 finisce con una finestra vuota.
- Scrittura degli hook agenti (Fase 3).
- Rendering del terminale oltre a quanto già fa la Fase 0 (Fase 2).
- Diff a due colonne e lettura dei layout di workspace: le tabelle esistono e i
  dati sono importati, ma nessun codice li legge prima delle Fasi 4 e 6.

## Questioni aperte per le fasi successive

- **Fase 3**: il Layer D della detection usa `proc_listchildpids`/`proc_name` di
  libproc. In Node serve un modulo nativo o uno shell-out; è l'unico segnale che
  intercetta gli agenti senza convenzione di titolo usabile, quindi non è
  opzionale.
- **Fase 4**: `workspaceLayout.payload` va riprogettato — la forma attuale
  serializza tipi SwiftUI.
- **Fase 5**: `chatItem.payload` idem, dipende dal driver ACP.
