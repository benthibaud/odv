# Cahier des Charges & Architecture Technique — Traitement de Texte Moderne

> **Projet :** Traitement de texte sémantique et multi-supports adapté à notre époque.  
> **Manifeste :** *« Ne construisons pas un traitement de texte des années 90 simulant une machine à écrire sur du papier ; concevons un instrument d'écriture et de publication moderne, sémantique, sobre et multi-supports. »*

---

## 1. Vision, Philosophie & Objectifs

### 1.1. Constat & Problématique des outils historiques
* **La dette des suites bureautiques des années 90 (Word, LibreOffice Writer) :** L'interface incite à la mise en forme locale anarchique (« coloriage manuel » au coup par coup), créant un code interne pollué (milliers de `<span>`/`<div>` invisibles) et des documents décousus impossibles à maintenir.
* **Le piège du skeuomorphisme papier :** Réduire un document à une suite rigide de pages A4 le rend inconfortable à lire sur smartphone, tablette ou liseuse.
* **Les limites du Markdown brut :** S'il est sobre et léger, le Markdown standard ne permet pas nativement les mises en page élaborées (tableaux fusionnés, pagination A4 normée, encadrés de rubriques, en-têtes et pieds de page au millimètre).

### 1.2. La Réponse : Le *Single-Source Publishing* Sémantique
Un document unique (la **Source de Vérité**) conserve une structure logique pure. C'est la **feuille de style** qui adapte automatiquement le document au support de destination :
* **Page imprimable A4 / Letter :** Rendu paginé au millimètre avec en-têtes, césures et numérotation.
* **Livre numérique (ePub) :** Lecture confortable sur liseuse/smartphone avec mémorisation de la progression.
* **Flux Web / Continu :** Lecture fluide responsive sans coupure artificielle.
* **Échange bureautique (ODT) :** Export et réimport propres pour LibreOffice.

```
                    ┌───────────────────────────────┐
                    │     DOCUMENT SOURCE UNIQUE    │
                    │   (Arbre Sémantique / AST)    │
                    └───────────────┬───────────────┘
                                    │
       ┌────────────────────────────┼────────────────────────────┐
       ▼                            ▼                            ▼
┌──────────────┐             ┌──────────────┐             ┌──────────────┐
│  VUE PAPIER  │             │  VUE LISEUSE │             │   VUE WEB    │
│  (A4 / PDF)  │             │   (ePub 3)   │             │ (Responsive) │
└──────────────┘             └──────────────┘             └──────────────┘
```

---

## 2. Pile Technique & Choix d'Ingénierie

### 2.1. Langage de Programmation & Moteur Cœur : **Rust**
* **Pourquoi Rust ?**
  * **Sécurité mémoire absolue sans Ramasse-Miettes (*Zero-Cost Safety*) :** Élimine d'office les fuites et les plantages mémoire (*segfaults*).
  * **Performances & Vitesse instantanée :** Temps de démarrage inférieur à **20 ms**, consommation RAM minimale (< 30 Mo), manipulation fluide de documents volumineux.
  * **Structure de données texte :** Utilisation d'une **Piece Table** ou d'un **Rope** pour des opérations de saisie/suppression à coût constant $\mathcal{O}(\log N)$.
  * **Compilation multi-cibles :** Binaire natif ultra-rapide sous Linux / Windows / macOS, et compilation WebAssembly (**WASM**) pour exécution directe dans le navigateur.

### 2.2. Moteur de Rendu & Typographie
* **Rendu Typographique :** Intégration de **HarfBuzz** et **Cosmic-Text / FreeType** pour le positionnement précis des glyphes et la gestion multilingue/bidirectionnelle (arabe, hébreu).
* **Mise en page papier :** Standard **CSS Paged Media** (`@page`, `break-inside: avoid;`, compteurs de pages dynamiques).
* **Règles de design :**
  * 1 à 3 polices de caractères maximum par document.
  * Échelle typographique harmonique modulaire ($1.25$ ou $1.333$).
  * Séparation stricte structure / style.

---

## 3. Matrice Fonctionnelle

### 3.1. Édition & Saisie Sémantique Contrainte (*Guard Rails*)
* **Interdiction stricte des styles locaux sauvages :** Aucun sélecteur de police ou de taille arbitraire à la volée. Toute modification visuelle s'applique à l'échelle d'une règle sémantique ou d'un thème.
* **Éradication des balises génériques :** Zéro `<div>` ou `<span>` arbitraire.
* **Palette de conteneurs sémantiques autorisés :**
  * Titres hiérarchiques (`h1` à `h5`).
  * Paragraphes (`p`), Citations (`blockquote`), Séparateurs (`hr`).
  * Listes (`ul`, `ol`, `li`).
  * Tableaux structurés (`table`, `thead`, `tbody`, `tr`, `th`, `td`).
  * Sections & Articles typés (`<article class="rubrique-edito">`, `<article class="rubrique-enquete">`).
  * Encadrés périphériques (`<aside class="encadre-focus">`, `<aside class="encadre-chiffres">`, `<aside class="encadre-pub">`).
  * Figures & Médias légendés (`<figure>`, `<figcaption>`).
* **Inspecteur Sémantique en direct :** Visualisation en temps réel du fragment HTML épuré et de la règle CSS gouvernant le bloc sous le curseur.

### 3.2. Typographie & Correction Linguistique
* **Correcteur orthographique natif :** Exploitation directe du moteur du navigateur et de l'OS via `spellcheck="true"` et `lang="fr"` (zéro dictionnaire lourd à embarquer, respect des mots personnalisés de l'utilisateur).
* **Micro-moteur typographique automatique (à la saisie) :**
  * Insertion automatique de l'espace insécable avant les ponctuations doubles (`:`, `;`, `!`, `?`).
  * Remplacement automatique des guillemets droits `"..."` par des guillemets français `« »`.
  * Conversion des tirets doubles `--` en tirets cadratins `—`.

### 3.3. Bibliothèque d'AutoTextes Standards (`F3` / `/`)
Déclenchement instantané par raccourci court + touche **`F3`** (ou **`Tab`** / commande slash `/`) :

| Raccourci | Bloc sémantique inséré | Rendu & Utilité |
| :--- | :--- | :--- |
| **`fenetre`** | `<address class="destinataire-fenetre only-print">` | Positionnement au millimètre pour enveloppes postales à fenêtre (norme DL $110 \times 220\text{ mm}$). Immuable au pliage. |
| **`courrier`** | Gabarit complet de lettre officielle | En-tête avec logo, bloc expéditeur, zone fenêtre, date, objet, corps, formule de politesse et pied de page légal. |
| **`focus`** | `<aside class="encadre-focus">` | Encadré teinté avec titre et icône pour mettre en valeur une notion clé. |
| **`edito`** | `<article class="rubrique-edito">` | Bloc éditorial avec photo d'auteur, lettrine et signature. |
| **`facture`** | `<table>` comptable structuré | Tableau pré-rempli avec quantités, prix unitaires, TVA et totaux HT/TTC. |
| **`sig`** | Formule de politesse standard | Formule + Nom + Titre / Qualité. |
| **`dt`** | Faux texte sémantique | 2 paragraphes de *Lorem Ipsum* balisés proprement pour tester la mise en page. |

* **Stockage :** Fichier de configuration `autotext.json` simple, modifiable et partageable en équipe.

### 3.4. Gestion de la Visibilité Multi-Supports & RGPD
* **Classes utilitaires contextuelles :**
  * `.only-print` : Visible uniquement sur papier, PDF A4 et courrier ODT.
  * `.only-web` : Visible uniquement en affichage Web et newsletters.
  * `.only-epub` : Visible uniquement sur liseuses.
* **Sas de sécurité & Purge RGPD :** Lors de la génération d'un document Web ou d'une newsletter publique, le moteur **purgera physiquement** les données privées (adresses postales, numéros LRAR) pour éviter toute fuite dans le code source.

### 3.5. Médias Modernes & Documents Vivants (*Living Documents*)
* **Images & Animations AVIF / HEIF :**
  * Support du format **AVIF** et **AVIF animé** (poids 10 fois inférieur au GIF, qualité supérieure).
* **Passerelle Hybride Statique $\leftrightarrow$ Interactif :**
  * **Dans le PDF ou l'ODT numérique :** L'image fixe (PNG nette) est **cliquable** et ouvre directement l'animation AVIF complète dans le navigateur par défaut de l'utilisateur.
  * **Sur la feuille A4 imprimée :** Présence automatique d'un lien court / mini QR-code sous la figure pour lancer l'animation sur smartphone.
  * **Sur le Web / Liseuse :** Animation directement jouée en place avec bouton lecture/pause.
* **Îlots JavaScript légers :** Quiz pour étudiants, simulateurs ou graphiques explorables avec dégradation automatique en capture statique lors de l'impression.

---

## 4. Hub de Diffusion & Interopérabilité

```
┌────────────────────────────────────────────────────────────────────────┐
│                        HUB DE DIFFUSION DU DOCUMENT                    │
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│  📘 LECTURE IMMÉDIATE                                                  │
│  [ 📱 Ouvrir en ePub ]  ──► Pour liseuses et smartphones (reprise page)│
│  [ 🌐 Ouvrir en Web  ]  ──► Pour affichage continu responsive         │
│                                                                        │
│  📄 BUREAUTIQUE                                                        │
│  [ 📝 Ouvrir en .ODT ]  ──► Échange propre avec LibreOffice            │
│                                                                        │
│  🖨️ GÉNÉRATION PDF (Sélection du gabarit) :                           │
│  (•) Format A4 / Letter   ──► Impression papier et archivage officiel   │
│  ( ) Format Écran Mobile  ──► PDF vertical taillé pour smartphone       │
│  ( ) Format Écran 16:9    ──► Présentation paysage pour tablette        │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

### 4.1. Sas d'Importation Unidirectionnel (Word / Scans / RTF)
* **Principe de décontamination :** Le format `.docx` n'est accepté qu'à l'entrée. Le convertisseur extrait le texte sémantique et les tables, puis détruit toutes les balises propriétaires opaques, métadonnées cachées et styles parasites.

### 4.2. L'Aller-Retour ODT (*Round-Trip sans perte*)
* Lors de l'export en `.odt` pour un collègue sous LibreOffice, le fichier ZIP embarque un manifeste de métadonnées (`source-meta.json`).
* Au retour du document corrigé, le moteur réintègre les modifications textuelles du collègue tout en **nettoyant les styles bricolés manuellement** et en réappliquant automatiquement la charte graphique d'origine.

### 4.3. Document HTML Universel Autonome
* Diffusion possible sous forme d'un fichier unique `document.html` intégrant son CSS, ses polices et un micro-moteur d'exportation (< 30 Ko) permettant à n'importe quel destinataire de télécharger son `.odt`, `.epub` ou `.pdf` d'un simple clic depuis son navigateur sans rien installer.

---

## 5. Synthèse des Arbitrages Clés

| Composant | Choix retenu | Rationale |
| :--- | :--- | :--- |
| **Cœur Applicatif** | **Rust** | Sécurité mémoire, démarrage instantané (< 20 ms), consommation RAM minimale. |
| **Modèle Source** | **HTML5 Sémantique / AST** | Expressivité totale (articles, asides, tables), interopérabilité directe ePub/Web. |
| **Styles & Layout** | **CSS Paged Media** | Séparation stricte fond/forme, rendu A4 haute précision sans pollution de code. |
| **Orthographe** | **Navigateur / OS (`spellcheck`)** | Zéro poids additionnel, multilinguisme immédiat, conservation des dictionnaires utilisateur. |
| **Exports Cibles** | **ODT, PDF (x3), ePub 3, HTML** | Couverture intégrale des besoins (bureautique, archivage, mobile, web). |
