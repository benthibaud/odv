# Architecture Technique — `odv` (Open Document View)

> **Statut :** Socle validé après banc d'essai Blitz (20 nov. 2026)  
> **Langage :** Rust (`odv-core` + `odv-ui`)  
> **Licence cible :** Libre (compatible MIT / Apache-2.0)

---

## 1. Principes Directeurs & Découplage

1. **Source Unique Sémantique :** Un document `.odv` est un arbre sémantique pur (AST), sans mise en forme locale anarchique (*guard rails* : zéro `<div>`/`<span>` arbitraire, 1 à 3 polices max, échelle typographique modulaire).
2. **Séparation stricte `odv-core` / `odv-ui` :**
   - `odv-core` ne dépend **d'aucune** crate graphique (`blitz-*`, `winit`, `anyrender`). Toute la logique métier (AST, édition, undo/redo, AutoTextes, micro-typographie, `calc_model`, sérialisation `.odv`, exports/imports, i18n) y vit.
   - `odv-ui` est une façade mince chargée du fenêtrage (`winit`), du rendu HTML/CSS (`blitz-dom` + `blitz-paint` + `anyrender_vello_cpu` / `vello`), de la capture d'événements et du presse-papiers X11/Wayland.
3. **Zéro dépendance GTK, zéro dépendance Qt, zéro navigateur externe :** Fonctionne nativement sous X11 (y compris environnements légers type antiX) et Wayland.
4. **Internationalisation dès la ligne 1 :** Langue pivot anglaise stricte dans le code source (`odv-core` / `odv-ui`), traduction française active (`fr.po` / Gettext) dès le premier jour.

---

## 2. Architecture Workspace Cargo

```text
odv/
├── Cargo.toml                  # Workspace Rust
├── crates/
│   ├── odv-core/               # Cœur métier pur (zéro UI)
│   │   ├── ast/                # Nœuds sémantiques, curseur, sélection, Piece Table / Rope
│   │   ├── calc/               # Feuilles de calcul Lotus Improv (dimensions nommées + DAG)
│   │   ├── format/             # Lecture/écriture conteneur ZIP .odv (meta.json, document.json, styles/, medias/)
│   │   ├── interop/            # Export ODT/ePub3/HTML/PDF, import .docx décontaminé, round-trip ODT
│   │   ├── typo/               # Micro-typographie FR/EN, AutoTextes (autotext.json)
│   │   └── i18n/               # Catalogue Gettext (pivot EN, fr.po)
│   └── odv-ui/                 # Interface native Blitz + Winit
│       ├── app.rs              # Boucle winit + BlitzApplication
│       ├── clipboard.rs        # Instance arboard::Clipboard persistante (correctif X11)
│       ├── editor_bridge.rs    # Traduction événements clavier/souris -> mutations AST odv-core
│       ├── projection.rs       # Projection AST + feuille CSS active -> DOM Blitz
│       └── views/              # 4 vues caméléon (Carte mentale, Paginé A4, ePub, Écran)
├── styles/                     # Feuilles CSS standards par défaut (tokens.css, document.css, epub.css, presentation.css)
└── locales/                    # Fichiers .pot / fr.po / .mo
```

---

## 3. Choix du Socle UI : Blitz (Validé par Banc d'Essai)

Le banc d'essai `blitz-poc` (20 nov. 2026) a validé sur Ubuntu 24.04 X11 (`rustc 1.99.0`) :
- **Compilation sans `-dev` système :** via `fontique` avec `features = ["fontconfig-dlopen"]` (`libfontconfig.so.1` chargée dynamiquement).
- **Exécution sans GTK/Qt :** `ldd` limité à `libc`, `libm`, `libgcc_s`.
- **Moteur CPU par défaut :** `anyrender_vello_cpu` (`softbuffer_window_renderer` + `multithreading`), garantissant la compatibilité sur machines légères sans Vulkan requis.
- **Rendu & bascule CSS instantanée :** `Stylo` + `Taffy` + `Parley` gèrent le basculement dynamique des feuilles CSS sans latence.

### Points d'attention traités dans `odv-ui`
1. **Presse-papiers X11 (`arboard`) :** `blitz-shell` recréant/droppant l'objet `Clipboard` à chaque copie, X11 perd la propriété de la sélection en quelques millisecondes. `odv-ui` maintient un `arboard::Clipboard` **persistant** sur toute la durée de vie de l'application (et pourra soumettre un correctif upstream à `DioxusLabs/blitz`).
2. **Édition sémantique (absence de `contenteditable` dans Blitz) :** `odv-ui` ne s'appuie pas sur `<textarea>` pour le document. Les frappes clavier, raccourcis (`Ctrl+B`, `F3`, `/`) et clics sont interceptés et traduits en commandes sur l'AST `odv-core`, qui met à jour la projection HTML/CSS dans `blitz-dom`.
3. **Indépendance vis-à-vis de Blitz :** Versions épinglées par `Cargo.lock` ; toute contribution amont est limitée aux bugs transversaux (presse-papiers, IME, rendu) sans dépendre de la roadmap Dioxus.

---

## 4. Format de Fichier `.odv`

Archive ZIP standard (`mon_document.odv`) :
- `meta.json` : Métadonnées document (titre, auteur, langue, version schéma, polices autorisées).
- `document.json` : AST sémantique pur (nœuds typés, attributs de visibilité `.only-print` / `.only-web` / `.only-epub`, notes de carte mentale, sauts de page/écran, blocs `calc_model`).
- `styles/` : Feuilles CSS standards directement éditables (`tokens.css`, `document.css`, `epub.css`, `presentation.css`).
- `medias/` : Images/animations **AVIF** (+ fallback PNG statique pour PDF/ODT).

---

## 5. Les 4 Vues Caméléon sur la Source Unique

| Vue | Source exploitée | Règles clés |
| :--- | :--- | :--- |
| **1. Carte Mentale** | Arbre des titres `h1`–`h5` + notes courtes | Réorganisation structurelle de branches ; notes non imprimées. |
| **2. Document Paginé / ODT** | AST complet + `document.css` | Format A4/Letter, en-têtes/pieds, enveloppe fenêtre DL, coupures de page, export `.odt` et PDF imprimable. |
| **3. Livre Numérique (ePub 3)** | AST + `epub.css` | Chapitrage sémantique, flux adaptatif, reprise de lecture. |
| **4. Écran / Présentation** | AST découpé en écrans + `presentation.css` | Ratios 16:9 ou portrait mobile, transitions, purge physique RGPD des blocs `.only-print` à l'export HTML autonome. |

---

## 6. Moteur de Calcul Intégré (`calc_model`)

Inspiré de **Lotus Improv** et des principes de performance de **Gnumeric** (`rapport_gnumeric_architecture_performance.md`) :
- **Dimensions nommées & formules lisibles :** `Marge = Revenus - Charges` plutôt que des coordonnées `B2:B14`.
- **Représentation creuse (sparse) & Graphe DAG :** Recalcul incrémental strictement limité aux nœuds dépendants.
