# 03 — Modèle de menaces du labo

> Analyse de sécurité du labo IoT : pour chaque objet, son archétype dans le monde
> réel, sa classe de vulnérabilité, les piliers de sécurité touchés, les chemins
> d'attaque, le mapping vers l'**OWASP IoT Top 10 (2018)**, et la remédiation prévue
> par la couche **IAM / zero-trust** (mois 5).
>
> ⚠️ Toutes les vulnérabilités décrites ici sont **volontaires** et confinées à un
> **labo isolé**. Aucun objet n'est exposé à Internet.

---

## Référentiel utilisé : OWASP IoT Top 10 (2018)

C'est la liste de référence des risques IoT les plus critiques. Chaque objet du labo
est mappé sur les items concernés.

| Code | Item |
|---|---|
| **I1** | Weak, Guessable, or Hardcoded Passwords |
| **I2** | Insecure Network Services |
| **I3** | Insecure Ecosystem Interfaces |
| **I4** | Lack of Secure Update Mechanism |
| **I5** | Use of Insecure or Outdated Components |
| **I6** | Insufficient Privacy Protection |
| **I7** | Insecure Data Transfer and Storage |
| **I8** | Lack of Device Management |
| **I9** | Insecure Default Settings |
| **I10** | Lack of Physical Hardening |

Source : OWASP Internet of Things Project (voir §Sources).

---

## Vue d'ensemble : couverture de la triade CIA

Le principe fondateur du labo : chaque objet est l'**archétype** d'une catégorie
d'objets réels, et porte une **classe de vulnérabilité distincte**. Ensemble, les
trois objets couvrent les trois piliers de la sécurité.

| Objet | Archétype réel | Rôle | Pilier CIA menacé | Impact principal |
|---|---|---|---|---|
| Caméra (ESP32-CAM) | Caméra IP, interphone vidéo | Expose des données | **Confidentialité** | Fuite d'information |
| Capteur (ESP32 + DHT22) | Télémétrie domotique / industrielle | Produit des données | **Intégrité** (+ Confidentialité) | Fausse télémétrie |
| Actionneur (ESP32 + LED/bouton) | Serrure, relais, disjoncteur | Agit sur le monde physique | **Disponibilité / Intégrité** | Action non autorisée |

---

## Objet 1 — La caméra (ESP32-CAM)

**Archétype réel.** Caméras IP, interphones vidéo, moniteurs bébé, webcams — une
catégorie massivement ciblée dans le monde réel (botnets type Mirai, construits en
partie sur des caméras à identifiants par défaut).

**Classe de vulnérabilité.** Exposition de données sensibles sans contrôle d'accès :
le flux vidéo est accessible sans authentification, et transite sans chiffrement.

**Pilier CIA.** Confidentialité (le flux révèle de l'information privée).

**Mapping OWASP IoT Top 10.**

| Code | Pourquoi |
|---|---|
| **I3** — Insecure Ecosystem Interfaces | Interface web / flux accessible sans authentification. |
| **I9** — Insecure Default Settings | Aucune protection activée par défaut. |
| **I6** — Insufficient Privacy Protection | Le flux vidéo est une donnée à caractère privé, non protégée. |
| **I7** — Insecure Data Transfer and Storage | Flux transmis en clair. |

**Chemins d'attaque.**
1. Découverte : le scanner de ports identifie le port HTTP du flux.
2. Accès : visionnage direct du flux, sans identifiant.
3. (Optionnel) Extraction du firmware pour comprendre le fonctionnement.

**Outils du labo impliqués.** Scanner (mois 1), extraction firmware + Ghidra (mois 2).

**Remédiation IAM (mois 5).** Placer le flux derrière un point d'application exigeant
une identité de confiance (mTLS / jeton) ; chiffrement en transit via TLS.

---

## Objet 2 — Le capteur (ESP32 + DHT22)

**Archétype réel.** Toute la télémétrie domotique et industrielle (IIoT) : sondes de
température, d'humidité, de pression, compteurs. L'épine dorsale de l'IoT.

**Classe de vulnérabilité.** Donnée **non authentifiée** et **non chiffrée**. Deux
problèmes distincts : elle circule en clair (interception), et rien ne prouve qu'elle
provient du vrai capteur (falsification).

**Pilier CIA.** Intégrité (falsification de la mesure) + Confidentialité (interception).

**Impact réel.** Un capteur de température falsifié dans un entrepôt pharmaceutique ou
une installation industrielle peut masquer un incident physique. La falsification de
télémétrie a des conséquences matérielles.

**Mapping OWASP IoT Top 10.**

| Code | Pourquoi |
|---|---|
| **I2** — Insecure Network Services | Le broker MQTT accepte les connexions anonymes. |
| **I7** — Insecure Data Transfer and Storage | Mesures publiées en clair (MQTT non chiffré). |
| **I9** — Insecure Default Settings | Aucune authentification configurée. |

**Chemins d'attaque.**
1. Interception : le sniffer lit la donnée MQTT en clair.
2. Falsification en transit : MITM altère la mesure.
3. Injection : publication directe de fausses mesures sur le broker (anonyme).

**Outils du labo impliqués.** Sniffer (mois 2), MITM + fuzzer (mois 3).

**Remédiation IAM (mois 5).** mTLS entre capteur et broker (authenticité de la source
+ chiffrement) ; seul un objet porteur d'un certificat valide peut publier.

---

## Objet 3 — L'actionneur (ESP32 + LED verte/rouge + bouton)

**Archétype réel.** Serrures connectées, relais industriels, vannes, disjoncteurs,
alarmes — tout ce qui **exécute une action physique** sur commande. Les LED
verte/rouge représentent un état (ouvert/fermé, actif/coupé, armé/désarmé).

**Classe de vulnérabilité.** Exécution de **commandes non autorisées** + **mot de
passe codé en dur** dans le firmware. L'objet obéit à tout ordre reçu, sans vérifier
l'identité de l'émetteur.

**Pilier CIA.** Disponibilité / Intégrité — avec **impact physique direct** (c'est la
classe la plus grave).

**Impact réel.** Un flux vidéo volé est abstrait ; une serrure qui se déverrouille
seule est immédiatement compréhensible. C'est le cas d'école le plus parlant.

**Mapping OWASP IoT Top 10.**

| Code | Pourquoi |
|---|---|
| **I1** — Weak, Guessable, or Hardcoded Passwords | Mot de passe en dur dans le firmware. |
| **I2** — Insecure Network Services | Topic de commande MQTT sans authentification. |
| **I9** — Insecure Default Settings | L'objet exécute toute commande par défaut. |

**Chemins d'attaque.**
1. Reconnaissance : sniffer/MITM identifie le topic de commande et le format (`ON`/`OFF`).
2. Détournement : publication d'un faux ordre (`OFF`) sans autorisation → l'objet obéit.
3. (Variante) Altération d'un ordre légitime en transit via MITM.
4. (Appoint) Extraction firmware → récupération du mot de passe en dur (Ghidra).

**Outils du labo impliqués.** Sniffer + MITM + fuzzer (mois 2-3), Ghidra (mois 2).

**Remédiation IAM (mois 5).** L'actionneur n'accepte plus que des ordres signés par une
identité de confiance (mTLS / jeton). L'attaque « faux OFF » **échoue** une fois l'IAM
en place. C'est la démo avant/après du projet.

---

## Menaces transversales (infrastructure)

Au-delà des objets, le labo illustre des faiblesses systémiques :

| Code OWASP | Faiblesse dans le labo | Chemin d'attaque |
|---|---|---|
| **I2** — Insecure Network Services | Broker Mosquitto en accès anonyme, exposé sur le réseau. | Connexion directe au broker, lecture/écriture de tous les topics. |
| **I8** — Lack of Device Management | Aucun cycle de vie : pas de provisionnement ni de révocation d'identité. | Un objet compromis le reste indéfiniment. |
| **I10** — Lack of Physical Hardening | Accès physique au port série / dump de la flash possible. | Extraction du firmware → secrets en dur. |
| **I4** — Lack of Secure Update Mechanism | Pas de mise à jour signée du firmware. | Firmware modifiable / non vérifié. |
| **I5** — Use of Insecure or Outdated Components | Bibliothèques IoT sans durcissement. | Surface d'attaque élargie. |

---

## Chemins d'attaque : la chaîne complète (kill chain)

Les attaques du labo suivent une progression type, chacune portée par un outil **codé
à la main** :

1. **Reconnaissance** → *scanner de ports* : cartographier les objets et leurs services.
2. **Interception** → *sniffer* : lire le trafic MQTT en clair.
3. **Analyse** → *Ghidra* : extraire un firmware, retrouver un secret en dur.
4. **Exploitation** → *fuzzer* + *MITM* : injecter de fausses données, détourner l'actionneur.
5. **Mouvement latéral (test)** → vérifier que la segmentation empêche l'objet
   compromis d'atteindre le réseau de confiance.

---

## Les principes cachés (concepts démontrés)

- **La confiance implicite est le défaut originel de l'IoT.** Les objets font confiance
  à quiconque parle sur le réseau (MQTT anonyme en est l'incarnation). C'est le problème
  que le **zero-trust** résout.
- **Identité ≠ connectivité.** Aujourd'hui, être *sur le réseau* suffit pour agir.
  L'IAM dissocie les deux : il faut une *identité prouvée* (certificat), pas une simple
  connexion.
- **La segmentation limite le rayon d'explosion.** Le VLAN IoT isolé empêche un objet
  compromis de contaminer le reste (défense en profondeur).
- **L'impact physique rend l'IoT unique.** Une attaque peut ouvrir une porte ou couper
  un service — pas seulement voler de la donnée.
- **Le moindre privilège.** Un objet ne devrait pouvoir faire *que* ce dont il a besoin
  (permissions par objet, jetons courts).

---

## Remédiation globale : la couche IAM / zero-trust (mois 5)

| Mesure IAM | Classe de risque corrigée | Items OWASP adressés |
|---|---|---|
| Identité machine par **certificat** (un par objet) | Secrets partagés / en dur | I1, I9 |
| **mTLS** (authentification mutuelle + chiffrement) | Données en clair, source non prouvée, services ouverts | I2, I7 |
| **Autorité de certification** avec clé racine hors ligne | Gestion des clés / ancre de confiance | I1, I8 |
| **Fournisseur d'identité** (jetons courts, permissions par objet) | Accès excessif, absence d'autorisation | I2, I9 |
| **Révocation automatique** déclenchée par l'IDS | Cycle de vie d'identité | I8 |

**Démonstration avant/après.** Avant : un attaquant publie un faux `OFF` et détourne
l'actionneur. Après (IAM en place) : l'ordre non signé est rejeté ; si une identité est
compromise, l'IDS la détecte et déclenche sa révocation → l'objet est coupé, en direct.

---

## Sources

- OWASP Internet of Things Project — IoT Top 10 (2018) :
  https://owasp.org/www-project-internet-of-things/
- OWASP IoT Top 10 2018 Mapping Project :
  https://github.com/scriptingxss/OWASP-IoT-Top-10-2018-Mapping
- Document de référence OWASP IoT Top 10 2018 (PDF) :
  https://owasp.org/www-pdf-archive/OWASP-IoT-Top-10-2018-final.pdf