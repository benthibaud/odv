# Notes de Cadrage — Projet `odv` (Open Document View)

> Document de passation pour démarrer une nouvelle session directement depuis le dossier `odv/`.  
> Références historiques antérieures (15 sept. 2026) rapatriées dans ce dossier : `./cahier_des_charges_traitement_de_texte_moderne.md` et `./rapport_gnumeric_architecture_performance.md` (origine : `/media/ben/Amovible/Antigravity/profile/antigravity/brain/d2796729-3bf6-4818-b89a-22d0ee33faaa/`).

---

## 1. Concept Central & Source Unique
- **Principe :** Un seul document source sémantique (`.odv`) projetable et éditable sous plusieurs formes, sans duplication de contenu ni mise en forme locale anarchique (*guard rails* : zéro `<div>`/`<span>` arbitraire, 1 à 3 polices max, échelle typographique modulaire).
- **4 vues caméléon sur la Source Unique :**
  1. **Carte Mentale (façon XMind) :** Manipulation exclusive de l'arbre des titres (`h1`–`h5`), réorganisation de branches ; les infos complémentaires courtes jouent le rôle de **commentaires non imprimés**.
  2. **Document Paginé / ODT :** Édition de type traitement de texte gouvernée par feuille de style, format **A4 / Letter**, gestion des coupures de page imprimables, en-têtes/pieds de page, enveloppe à fenêtre DL. Sert pour **`.odt`** et **PDF imprimable**.
  3. **Livre Numérique (ePub 3) :** Chapitres, flux adaptatif liseuse/smartphone, reprise de lecture.
  4. **Écran / Présentation :** HTML découpé en écrans (**Desktop 16:9** ou **Smartphone portrait**), avec transitions entre écrans et apparitions progressives optionnelles ; export **HTML autonome** et **PDF écran**.

## 2. Format du Fichier `.odv` (Acté)
- **Conteneur ZIP structuré (`mon_document.odv`) :**
  - `meta.json`
  - `document.json` : AST sémantique pur (nœuds typés avec métadonnées transversales : notes de carte mentale, marqueurs de saut de page papier / écran, ordre d'apparition, classes `.only-print` / `.only-web` / `.only-epub` avec purge physique des blocs privés à l'export web).
  - `styles/` : Feuilles de style stockées en **`.css` standards et directement extractibles/importables** (`tokens.css`, `document.css`, `epub.css`, `presentation.css`). Traduction vers `styles.xml` uniquement au moment de l'export `.odt`.
  - `medias/` : Images/animations **AVIF** (+ PNG statique cliquable dans PDF/ODT et lien/QR code sur papier).
- **Feuilles de calcul intégrées (modèle Lotus Improv) :**
  - Blocs `calc_model` fondés sur des **dimensions nommées** et des **formules lisibles par variables nommées** (ex. `Marge = Revenus - Charges`), plutôt que des coordonnées `B2:B14`.
- **Interopérabilité :**
  - Import `.docx` unidirectionnel (sas de décontamination).
  - *Round-Trip* `.odt` avec capsule `source-meta.json` et nettoyage des styles bricolés au retour de LibreOffice.
  - AutoTextes (`F3` / `/`, `autotext.json`) et micro-typographie automatique.

## 3. Contraintes Techniques & Architecture (Actées)
- **Langage cœur :** **Rust** (`odv-core`), strictement découplé de l'interface.
- **Zéro dépendance GTK, zéro dépendance Qt :** Doit fonctionner aussi bien sur **antiX** (X11 léger) que sur des environnements de bureau modernes (X11 et Wayland).
- **Pas de dépendance à un navigateur externe** pour faire tourner l'application (écarté pour les mêmes raisons que WebKitGTK).
- **Pas de backend provisoire jetable** (leçon du projet *TiMonde* : éviter d'adapter l'UI dans un second temps après que la structure a pris des biais).
- **Internationalisation (i18n) dès la ligne 1 (leçon *TiMonde*) :**
  - **Langue pivot du code source : Anglais strict** (zéro chaîne française en dur dans le code).
  - **Traduction active dès le départ : Français (`fr.po` / Gettext) uniquement** pour les tests, sans traduire 55 langues avant maturité.

## 4. Environnement Vérifié & Prochaine Action Immédiate
- **Machine :** Ubuntu 24.04 (`x86_64`, Linux `7.0.0-43-generic`), session active **`x11`** (`DISPLAY=:0.0`).
- **Rust :** `rustc 1.99.0` (2026-09-28), `cargo 1.99.0` (2026-08-27), `cc`, `pkg-config`, `vulkaninfo` présents (pas de `-dev` `fontconfig`/`freetype2`/`vulkan` dans `pkg-config`).
- **Décision en suspens avant de rédiger `ARCHITECTURE.md` et `ROADMAP.md` :**
  - Réaliser un **banc d'essai éliminatoire (micro-PoC isolé)** sur **Blitz** (`winit` + `Stylo`/`Taffy`/`Parley` + `Vello`/`vello_cpu`, zéro GTK/Qt, X11 & Wayland natifs) dans `scratch/temp/blitz-poc/` pour vérifier :
    1. compilation et exécution sans GTK/Qt sur la session X11 (et mode CPU) ;
    2. saisie de texte française (accents / dead keys, sélection, copier-coller) et bascule de feuilles CSS.
  - Si Blitz passe $\rightarrow$ validation comme socle UI définitif ; sinon $\rightarrow$ bascule immédiate avant toute ligne d'`odv` sur une pile Rust native mature (`winit` + `cosmic-text`).
