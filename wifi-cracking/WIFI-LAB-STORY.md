# LAB-STORY — Entrée sur le réseau (capture & crack WPA2)

> Fiche normalisée destinée à l'assemblage du rapport final du lab `sec-lab-iot`.
> Même structure pour tous les sous-projets. Ton factuel, phrases réutilisables
> telles quelles dans un rapport.

## 1. Nom & rôle (une phrase)
Brique d'**accès initial** : obtenir un pied sur le réseau WiFi `IoT-Lab` en capturant
puis en cassant hors ligne le handshake WPA2-PSK, afin de rejoindre le réseau qui
héberge les objets connectés du lab.

## 2. Place dans le récit
- **Phase :** entrée réseau (tout premier maillon de la *kill chain*)
- **Vient après :** rien — c'est le point de départ, l'attaquant part de l'extérieur
  sans aucun accès.
- **Mène à :** la phase de **reconnaissance interne** et d'**exploitation des objets**
  (une fois sur le réseau, les ESP32 et autres devices deviennent atteignables et
  interrogeables).

## 3. Vulnérabilité ciblée
- **Faille :** WPA2-PSK autorise un **crack hors ligne par dictionnaire** — le 4-way
  handshake capturé sert d'oracle pour tester des mots de passe sans limite ni
  détection ; combiné à une **passphrase devinable** (`LabIoT2026`, motif
  `Mot+Année`), l'accès tombe.
- **Item OWASP IoT (si applicable) :** **I1 — Weak, Guessable, or Hardcoded Passwords**
  (OWASP IoT Top 10 2018). Définition OWASP : usage d'identifiants facilement forçables
  par brute-force ou publiquement disponibles, donnant un accès non autorisé.
- **Pilier CIA touché :** **Confidentialité** en premier (accès non autorisé au réseau
  et au trafic), avec un impact indirect sur **Intégrité** et **Disponibilité** une
  fois l'attaquant à l'intérieur.

## 4. Ce que j'ai fait
- **Approche :**
  1. Test de faisabilité matériel (mode monitor sur carte Intel 9560 réputée instable,
     validé empiriquement).
  2. Reconnaissance (`airodump-ng`) → identification de `IoT-Lab`, BSSID et canal.
  3. Capture du handshake verrouillée sur le canal + BSSID, déclenchée en
     **redémarrant l'ESP32** (reconnexion → handshake rejoué) plutôt que par
     désauthentification.
  4. Crack hors ligne (`aircrack-ng` + dictionnaire).
  5. Connexion à `IoT-Lab` avec le mot de passe retrouvé.
- **Choix techniques :** suite **aircrack-ng** (airmon-ng, airodump-ng, aircrack-ng),
  `iw` / `nmcli` côté système, **Wireshark** pour la vérification du handshake.
- **Pourquoi ces choix :** **outil standard reconnu, non recodé** — ré-implémenter un
  cracker WPA2 serait hors sujet (c'est le pendant « j'utilise Ghidra » des autres
  briques). La valeur du volet est dans la **compréhension de la chaîne** et l'analyse
  défensive, pas dans l'outillage. La capture par **reboot de l'ESP32** est un choix
  assumé : elle contourne le fait que les cartes Intel injectent mal, en exploitant le
  fait que je contrôle le client — pas besoin d'injection.

## 5. Résultat & preuve
- **Ce qui s'est passé :** chaîne complète réussie. Handshake capturé et vérifié
  (`WPA (1 handshake)` + 4 messages EAPOL dans Wireshark), mot de passe retrouvé
  (`KEY FOUND! [ LabIoT2026 ]`), connexion au réseau établie avec IP obtenue.
- **Fichiers média associés :**
  - `interface-modes.png` — carte supporte le mode monitor
  - `monitor-check_setup.png` — bascule monitor + chipset Intel/iwlwifi
  - `iot-lab-scan-highlight.png` — scan, cible `IoT-Lab` surlignée
  - `handshake-live-eapol.png` — capture live, `EAPOL` dans Notes *(nom prévu)*
  - `wireshark-eapol-4messages.png` — les 4 messages du handshake *(nom prévu)*
  - `key-found.png` — capture-trophée `KEY FOUND!` *(nom prévu)*
  - `nmcli-connected.png` — connexion réussie + IP *(nom prévu)*
  - *(option)* GIF/vidéo de la séquence reboot ESP32 → `EAPOL` → `KEY FOUND!`

## 6. Ce que ça démontre (formulation entretien)
**Un réseau protégé par un unique secret partagé (PSK) n'est qu'un périmètre, et un
périmètre ne vaut que la force de son secret : parce que le handshake WPA2 permet de
tester les mots de passe hors ligne, toute passphrase devinable s'effondre face à un
dictionnaire — l'accès au réseau ne devrait donc jamais valoir confiance sur les
objets qu'il contient.** Autrement dit : ce volet illustre pourquoi la sécurité
« par la localisation réseau » est insuffisante et doit être remplacée par une
authentification **par device** (amorce du raisonnement zero-trust).

## 7. Lien vers la remédiation IAM
C'est le **bouclage central** du lab. Cette attaque ne donne qu'un accès *réseau* — or
la couche IAM du mois 5 fait en sorte que **l'accès réseau ne donne aucun accès aux
objets** :
- **mTLS** entre devices et services : même présent sur `IoT-Lab`, un attaquant sans
  certificat client valide ne peut pas établir de session avec un ESP32 — le secret
  WiFi cassé ne lui sert à rien au niveau applicatif.
- **Jetons courts + révocation** : un accès éventuellement obtenu est éphémère et
  révocable, là où une passphrase WiFi cassée resterait valable indéfiniment.
- **Principe démontré :** on passe d'un modèle « un mur, un mot de passe » (ce volet) à
  un modèle « chaque échange est authentifié et chaque identité est vérifiée »
  (couche IAM). L'attaque de ce mois-ci est précisément ce que la défense du mois 5
  neutralise.

## 8. Statut portfolio
- **Autonome ou intégré :** **intégré à `sec-lab-iot`**. L'étape n'a de sens que comme
  accès initial de la *kill chain* ; isolée, un crack de son propre WPA2 avec un
  dictionnaire minimal serait peu différenciant.
- **Lien si autonome :** N/A — vit dans le sous-dossier `wifi/` du dépôt
  `sec-lab-iot-iam` (voir `wifi/README.md`).
