# LAB-STORY — Extraction de firmware & configuration MQTT de l'actionneur (`actionneur1`)

> Fiche normalisée destinée à l'assemblage du rapport final du lab `sec-lab-iot`.
> Même structure pour tous les sous-projets. Ton factuel, phrases réutilisables
> telles quelles dans un rapport.

## 1. Nom & rôle (une phrase)
Extraction du firmware de l'ESP32 `actionneur1` et reconstitution de sa configuration
MQTT (identité + broker + topics) — démontre qu'un accès physique à la puce suffit à
récupérer tout le nécessaire pour usurper l'objet sur son broker.

## 2. Place dans le récit
- **Phase :** exploitation (accès physique + analyse statique de firmware).
- **Vient après :** la reconnaissance du réseau IoT segmenté (p. ex. balayage via
  `rmap`) qui identifie `actionneur1` comme cible, puis l'accès physique à la carte.
- **Mène à :** l'usurpation de l'actionneur sur le broker Mosquitto (en accès anonyme) à
  l'aide de la config extraite ; et motive directement la remédiation IAM (mTLS) du
  mois 5.

## 3. Vulnérabilité ciblée
- **Faille :** identité d'appareil faible et extractible — le `clientId` MQTT et toute la
  config de connexion (IP du broker, port, topics) sont codés en dur, récupérables par
  dump de flash, puis **forgeables** sur un broker en accès anonyme. Côté puce, la flash
  est entièrement extractible via l'interface série (ni *flash encryption* ni *secure
  boot*).
- **Item OWASP IoT :** **I10 — Lack of Physical Hardening** (extraction de la flash) ;
  **I2 — Insecure Network Services** (le broker accepte, en anonyme, une identité
  copiable). *I1 (Hardcoded Passwords) n'est volontairement pas retenu ici : un
  `clientId` est un identifiant, pas un secret d'authentification.*
- **Pilier CIA touché :** **Intégrité** (usurpation → fausses commandes/télémétrie) et
  **Disponibilité** (éviction du client légitime par collision de `clientId`), à partir
  d'une divulgation de la configuration (Confidentialité).

## 4. Ce que j'ai fait
- **Approche :** du plus simple au plus complexe. (1) Dump de la flash via `esptool` par
  le port série ; (2) `strings` sur le dump — le `clientId` sort en clair ; (3) analyse
  statique dans Ghidra : localisation du `clientId`, confirmation de son usage via ses
  références croisées (XREF), puis reconstitution de toute la config de connexion
  présente dans la même routine (broker, port, topics).
- **Choix techniques :** `esptool`, `strings`, `esp32_image_parser` (Tenable, puis fork
  `samyk` pour les correctifs), Ghidra 12.2 (support Xtensa natif, JDK 21), sous Fedora.
- **Pourquoi ces choix :** outils standards de l'écosystème ESP32/reverse (résultat
  crédible et reproductible). La passe `strings` avant Ghidra illustre qu'on ne sort
  l'artillerie lourde que si nécessaire. Le passage par un ELF est requis pour donner à
  Ghidra la carte mémoire, sans quoi les XREF ne se résolvent pas — or ce sont justement
  les XREF qui mènent du `clientId` à la routine de connexion, et donc à toute la config.

## 5. Résultat & preuve
- **Ce qui s'est passé :** toute la configuration MQTT de l'objet a été reconstituée par
  analyse statique — broker `192.168.50.182`, port `1883` (`0x75b`), `clientId`
  `actionneur1` (`0x3f400138`, 9 XREF), topics `labo/actionneur/commande` et
  `labo/actionneur/etat`. Le broker étant anonyme, ces seuls éléments suffisent à se
  connecter en se faisant passer pour l'objet, à l'évincer (comportement normatif MQTT)
  et à publier de fausses commandes/états.
- **Fichiers média associés :**
  - `dumping.png` — le `read_flash` d'`esptool` en cours (preuve de l'extraction).
  - `clientid-strings.png` *(optionnel)* — le `clientId` en clair dans `strings`.
  - `ghidra-clientId-found.png` — le `clientId` dans `.flash.rodata`, avec ses XREF (le « quoi »).
  - `ghidra-clientId-usage.png` — le `clientId` chargé et utilisé dans le code de connexion MQTT (le « où/comment »).

## 6. Ce que ça démontre (formulation entretien)
L'**identité** d'un objet IoT est extractible et forgeable : lire la flash ne donne pas
qu'un `clientId`, mais tout le nécessaire pour se brancher à sa place — adresse du
broker, port, topics. Pire, le protocole MQTT impose au broker de déconnecter un client
dont le `clientId` est réutilisé : un attaquant peut donc usurper l'objet (fausses
commandes/télémétrie) **et** le faire tomber hors ligne, simplement en se connectant avec
son identité. Une identité fondée sur une chaîne copiable, sur un service réseau qui ne
vérifie rien, n'est pas une identité : c'est une étiquette.

## 7. Lien vers la remédiation IAM
La couche IAM du mois 5 retire la racine du problème. On remplace l'identité fondée sur
une chaîne par une **identité machine par certificat (mTLS)** : copier le firmware ne
donne plus de quoi usurper l'objet, car il faudrait sa clé privée (stockable en matériel,
protégée par *flash encryption* + *secure boot* — ce qui ferme aussi I10). Côté broker,
on quitte l'anonyme pour exiger un certificat client, et une politique d'ACL restreint
chaque identité à ses propres topics — le `clientId` cesse d'être une frontière de
sécurité. Des jetons/sessions à durée de vie courte bornent toute fenêtre d'abus. Le
récit boucle d'une **identité statique extractible et forgeable** vers une **identité
cryptographique vérifiable, cloisonnée par appareil et révocable**.

## 8. Statut portfolio
- **Autonome ou intégré :** intégré à `sec-lab-iot` (volet d'appoint, ~5 % du projet).
- **Lien si autonome :** N/A — volet non autonome ; documentation interne dans
  `reverse/` (`reverse/README.md` pour le write-up détaillé).
