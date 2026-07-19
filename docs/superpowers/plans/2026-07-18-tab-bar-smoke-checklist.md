# Tab bar — smoke checklist manuale

Prerequisito: app avviata, un progetto con un worktree, 3+ tab miste
(terminale, chat, markdown).

- [ ] La tab bar appare sopra il terminale col worktree selezionato; con
      zero tab mostra solo il "+"; senza worktree selezionato è nascosta.
- [ ] Click su una tab nella bar → contenuto cambia e la sidebar evidenzia
      la stessa tab; click su una TabRow in sidebar → la bar si aggiorna.
- [ ] × in hover chiude la tab; su markdown dirty appare la conferma.
- [ ] "+" apre il menu (Nuovo Terminale / agenti / Chat · agente) e le voci
      creano la tab giusta.
- [ ] Doppio click sul titolo → rename inline (Invio conferma, Esc annulla);
      il nuovo titolo appare anche in sidebar.
- [ ] Context menu: Rinomina, Chiudi, Chiudi altre (disabilitato con 1 tab),
      Chiudi a destra (disabilitato sull'ultima).
- [ ] Drag di una tab nella bar → riordino; stesso drag su TabRow in
      sidebar; l'ordine coincide tra bar e sidebar e sopravvive al riavvio.
- [ ] Drag di una tab su un worktree diverso in sidebar → nessun drop.
- [ ] Con molte tab: la bar scrolla, la tab attiva torna visibile quando
      selezionata da sidebar/⌘n, appare il chevron con l'elenco completo.
- [ ] ⌘1..⌘n attiva la tab n; ⌘9 l'ultima; ⌃Tab / ⌃⇧Tab cicla con wrap.
- [ ] Agente in esecuzione in una tab non attiva → indicatore attività
      sulla tab (running dots / dot needs-input).
- [ ] Cambio tab ripetuto: i PTY restano vivi (processi non riavviati).
