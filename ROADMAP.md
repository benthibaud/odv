# Feuille de Route — `odv` (Open Document View)

> **Dernière mise à jour :** 20 nov. 2026  
> **Prérequis validé :** Banc d'essai Blitz (`scratch/temp/blitz-poc/`) réussi sur X11 avec `vello_cpu`.

---

## Protocole de Validation du Chemin Critique (Règle Transversale)
Pour chaque décision structurante à fort impact (« effet papillon ») :
1. **Options avant codage :** comparer les pistes possibles et leur impact sur les 4 vues et l'interopérabilité.
2. **Tests de contrat :** écrire des tests automatisés vérifiant les invariants métier (PASS/FAIL) avant de passer à la phase suivante.
3. **Non-régression :** maintenir l'intégralité des tests de contrat au vert dans `cargo test --workspace`.

---

## Phase 0 — Socle & Gouvernance (En cours)
- [x] Rapatriement des cahiers des charges et rapports d'architecture.
- [x] Banc d'essai éliminatoire Blitz (`fontconfig-dlopen`, zéro GTK/Qt, X11, saisie FR, bascule CSS).
- [x] Rédaction de `ARCHITECTURE.md` et `ROADMAP.md`.
- [x] Initialisation du dépôt Git (`benthibaud/odv`).
- [x] Initialisation du workspace Cargo (`odv-core` + `odv-ui`).
- [x] Mise en place i18n dès la ligne 1 (pivot anglais + `locales/fr.po`).

## Phase 1 — Noyau AST & Format `.odv` (`odv-core`)
- [x] Définition de l'AST sémantique (`h1`–`h5`, `p`, `blockquote`, `ul`/`ol`, `aside`, classes `.only-print`/`.only-web`/`.only-epub`/`.only-slide`, `PageBreak`).
- [x] Sérialisation / désérialisation du conteneur ZIP `.odv` (`meta.json` avec `schema_version`, `document.json`, `styles/*.css`, `medias/`).
- [x] Gestion des mutations AST (déplacement de branches Carte Mentale) et historique Annuler/Rétablir (*Undo/Redo* par snapshots `Arc`).
- [x] Tests de contrat sur `odv-core` (round-trip `.odv`, purge RGPD `.only-print`, séparation Présentation / Notes présentateur, déplacement de branche).

## Phase 2 — Éditeur Sémantique & Shell UI (`odv-ui`)
- [x] Fenêtre `winit` + `blitz-shell` + `anyrender_vello_cpu` (`fontconfig-dlopen`, `FontContext` partagé < 10 ms/frappe).
- [x] Correctif presse-papiers X11 (`arboard::Clipboard` persistant pour `Ctrl+C` / `Ctrl+V`).
- [x] Pont d'édition clavier/souris vers `odv-core` (saisie FR, sélection intra-bloc `Shift+←`/`→`, `Ctrl+←`/`→`, `Home`/`End`, `Ctrl+A`, `Delete`, `Ctrl+B`/`I`, niveaux `Ctrl+0..5`).
- [x] Micro-typographie automatique (`« »`, espaces insécables, `—`, `oe` $\rightarrow$ `œ`, profils régionaux `fr-FR` / `fr-CH` / `en`) et AutoTextes (`F3` / `/`, `autotext.json`).
- [x] Correcteur orthographique `spellbook` (100 % Rust / Nuspell) sur dictionnaires système Hunspell (`fr_FR`, `en_US`, zéro dépendance C).
- [x] Sélection souris exacte au caractère près, glisser-déposer et double-clic mot via `inline_layout_data` (Parley dans `blitz-dom`).
- [x] Bascule en direct des feuilles CSS (`tokens.css` + CSS de vue via `F2`).

## Phase 3 — Les 4 Vues Caméléon
- [ ] **Vue Écran / Présentation :** découpage en écrans (16:9 & portrait), export HTML autonome (< 30 Ko runtime).
- [x] **Vue Carte Mentale :** édition directe de l'arbre `h1`–`h5` (`Tab`, `Entrée`, `Ctrl+↑`/`↓`, `F4` replier/déplier), notes courtes non imprimées, import `.xmind` (XMind 8) et `.mm` (Freeplane/Docear/VYM).
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
