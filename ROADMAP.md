# Feuille de Route — `odv` (Open Document View)

> **Dernière mise à jour :** 20 nov. 2026  
> **Prérequis validé :** Banc d'essai Blitz (`scratch/temp/blitz-poc/`) réussi sur X11 avec `vello_cpu`.

---

## Phase 0 — Socle & Gouvernance (En cours)
- [x] Rapatriement des cahiers des charges et rapports d'architecture.
- [x] Banc d'essai éliminatoire Blitz (`fontconfig-dlopen`, zéro GTK/Qt, X11, saisie FR, bascule CSS).
- [x] Rédaction de `ARCHITECTURE.md` et `ROADMAP.md`.
- [x] Initialisation du dépôt Git (`benthibaud/odv`).
- [ ] Initialisation du workspace Cargo (`odv-core` + `odv-ui`).
- [ ] Mise en place i18n dès la ligne 1 (pivot anglais + `locales/fr.po`).

## Phase 1 — Noyau AST & Format `.odv` (`odv-core`)
- [ ] Définition de l'AST sémantique (`h1`–`h5`, `p`, `blockquote`, `ul`/`ol`, `table`, `article`, `aside`, `figure`, classes `.only-print`/`.only-web`/`.only-epub`).
- [ ] Sérialisation / désérialisation du conteneur ZIP `.odv` (`meta.json`, `document.json`, `styles/*.css`, `medias/`).
- [ ] Gestion des mutations AST, curseur, sélection et historique Annuler/Rétablir (*Undo/Redo*).
- [ ] Tests unitaires complets sur `odv-core` (round-trip `.odv`, purge RGPD `.only-print`).

## Phase 2 — Éditeur Sémantique & Shell UI (`odv-ui`)
- [ ] Fenêtre `winit` + `blitz-shell` + `anyrender_vello_cpu` (`fontconfig-dlopen`).
- [ ] Correctif presse-papiers X11 (`arboard::Clipboard` persistant pour `Ctrl+C` / `Ctrl+V` / `Ctrl+X`).
- [ ] Pont d'édition clavier/souris vers `odv-core` (saisie FR, `Ctrl+B`/`Ctrl+I`, hiérarchie titres).
- [ ] Micro-typographie automatique (`« »`, espaces insécables, `—`, `oe` $\rightarrow$ `œ`) et AutoTextes (`F3` / `/`, `autotext.json`).
- [ ] Bascule en direct des feuilles CSS (`tokens.css` + CSS de vue).

## Phase 3 — Les 4 Vues Caméléon
- [ ] **Vue Écran / Présentation :** découpage en écrans (16:9 & portrait), export HTML autonome (< 30 Ko runtime).
- [ ] **Vue Carte Mentale :** édition directe de l'arbre `h1`–`h5`, glisser-déposer de branches, notes courtes non imprimées.
- [ ] **Vue Document Paginé :** gabarits A4/Letter, enveloppe fenêtre DL, coupures de page, export PDF imprimable.
- [ ] **Vue Livre Numérique :** export ePub 3 valide.

## Phase 4 — Interopérabilité Bureautique & Médias
- [ ] Export `.odt` (traduction `styles/*.css` $\rightarrow$ `styles.xml`, capsule `source-meta.json`).
- [ ] Réimport `.odt` (*round-trip* avec nettoyage des styles locaux bricolés).
- [ ] Sas d'import `.docx` unidirectionnel (décontamination).
- [ ] Médias AVIF / AVIF animés avec fallback PNG cliquable et QR-code sur papier.

## Phase 5 — Feuilles de Calcul Intégrées (`calc_model`)
- [ ] Modèle dimensionnel nommé (style Lotus Improv).
- [ ] Évaluateur de formules lisibles avec graphe de dépendances DAG et stockage creux.
- [ ] Rendu tabulaire sémantique dans les 4 vues.
