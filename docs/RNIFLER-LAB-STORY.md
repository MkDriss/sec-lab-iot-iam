# LAB-STORY — rnifler

> Fiche normalisée destinée à l'assemblage du rapport final du lab `sec-lab-iot`.
> Même structure pour tous les sous-projets. Ton factuel, phrases réutilisables
> telles quelles dans un rapport.

## 1. Nom & rôle (une phrase)
rnifler est un sniffer réseau défensif écrit en Rust qui capture, réassemble et décode en clair le trafic MQTT 3.1.1 du broker du lab, en signalant chaque message qui viole la spécification.

## 2. Place dans le récit
- **Phase :** reconnaissance passive côté attaquant, observation côté défense : le même outil montre ce qu'un observateur du réseau peut lire, et fournit au défenseur la vision du trafic.
- **Vient après :** `rmap`, qui a découvert le broker et son port MQTT (1883) ouvert sur le réseau IoT.
- **Mène à :** l'attaque de l'homme du milieu sur MQTT, dont rnifler permet d'observer l'effet, puis l'IDS maison, dont il constitue la base prévue (ses événements deviendront la matière première des règles de détection).

## 3. Vulnérabilité ciblée
- **Faille :** le broker Mosquitto accepte des connexions MQTT sans chiffrement sur le port 1883 : identifiants (nom d'utilisateur et mot de passe du CONNECT), mesures des capteurs et commandes des actionneurs circulent en clair et sont lisibles par toute machine qui voit passer le trafic.
- **Item OWASP IoT (si applicable) :** I7 — Insecure Data Transfer and Storage (absence de chiffrement des données en transit). Secondairement I9 — Insecure Default Settings, le broker acceptant aussi les connexions anonymes.
- **Pilier CIA touché :** Confidentialité (identifiants et messages exposés). En défense, l'outil protège aussi l'Intégrité de sa propre analyse : il refuse les paquets que la machine destinataire rejetterait, pour ne pas afficher un trafic qui n'a jamais existé.

## 4. Ce que j'ai fait
- **Approche :** construction couche par couche, chaque étape testée avant la suivante :
  1. capture avec libpcap, filtre BPF exécuté dans le noyau (`tcp port 1883`), privilège limité à `CAP_NET_RAW` au lieu de root ;
  2. découpage Ethernet → IPv4/IPv6 → TCP ;
  3. décodeur MQTT 3.1.1 écrit à la main à partir de la spécification OASIS : 14 types de paquets, 19 sortes d'anomalies, référencées par l'exigence violée lorsqu'il en existe une ;
  4. réassemblage TCP par sens de connexion : messages coupés en plusieurs segments, retransmissions, trous, resynchronisation, connexions déjà ouvertes au démarrage ;
  5. résistance aux données hostiles : vérification de chaque longueur avant lecture, neutralisation des séquences d'échappement à l'affichage (CWE-150), vérification des checksums contre l'insertion, acceptation des RST selon la règle de Linux (RFC 5961), mémoire bornée ;
  6. enregistrement du trafic brut en fichiers pcap, avec rotation à 100 Mio, noms horodatés en UTC à la milliseconde et droits réservés au propriétaire ;
  7. 138 tests automatisés, 9 scénarios d'attaque ou de panne rejouables, intégration continue et publication de binaires x86_64 et ARM 64 bits.
- **Choix techniques :** Rust (version 1.88 minimum) ; crates `pcap` (libpcap), `etherparse` (en-têtes réseau), `clap` (ligne de commande) ; GitHub Actions pour la CI/CD ; validation contre Mosquitto, tcpdump et Wireshark.
- **Pourquoi ces choix :**
  - **décodeur MQTT recodé pour comprendre**, et parce qu'une bibliothèque cliente rejette un paquet invalide alors qu'un outil de défense doit expliquer pourquoi il l'est ;
  - **Rust**, pour lire des données potentiellement hostiles sans risque de corruption mémoire, et décoder les messages sans recopier les données (zéro-copie vérifiée à la compilation) ;
  - **`etherparse` pour les couches standard** (Ethernet, IP, TCP), bien connues et sans valeur pédagogique à réécrire ;
  - **brique indépendante du lab** (bibliothèque qui émet des événements, interface en ligne de commande séparée), pour être réutilisée telle quelle par l'IDS.

## 5. Résultat & preuve
- **Ce qui s'est passé :**
  - en direct sur le Raspberry Pi, rnifler décode le trafic réel des ESP32 : mesures du capteur toutes les 5 secondes, état de l'actionneur (`ON`/`OFF`, message retenu), échanges de maintien de connexion (PINGREQ/PINGRESP) toutes les 15 secondes ; les connexions déjà ouvertes au démarrage sont reprises sans fausse anomalie ;
  - un CONNECT portant des identifiants est décodé en clair : le mot de passe est masqué par défaut à l'affichage, et l'option `--show-secrets` le révèle, ce qui démontre l'exposition ;
  - une retransmission TCP réelle, survenue sur le Wi-Fi du lab, est reconnue : les 33 octets déjà reçus sont ignorés et le message suivant est décodé une seule fois ;
  - le scénario d'insertion montre qu'un faux paquet au checksum invalide est affiché par un outil qui ne vérifie rien, et écarté par rnifler, qui affiche le vrai message.
- **Fichiers média associés :** (dossier `public/projects/rnifler/pictures/` du portfolio)
  - disponibles : `usage-direct.png` (capture en direct, à renommer depuis `capture-with-on.png`), `capture-retransmission-reelle.png` (à renommer depuis `reconstitution.png`), `usage-rejeu.png`, `usage-anomalie.png`, `capture-message-coupe.png`, `capture-topic-piege.png`, `capture-longueur-mensongere.png`, `capture-insertion-avant-apres.png`, `capture-cargo-test.png`, `capture-release.png` ;
  - prévus : `session-complete.png` (CONNECT avec identifiants, mot de passe masqué), `usage-enregistrement.png` (liste des fichiers pcap et leurs droits), `usage-wireshark.png` (fichier enregistré ouvert dans Wireshark) ;
  - schémas (versions `.fr.png` et `.en.png`) : `schema-trajet`, `schema-filtre-bpf`, `schema-message-coupe`, `schema-trou-resynchronisation-1` à `-3`, `schema-publish-octets`, `schema-rotation`, `schema-menaces-parades`, `schema-insertion`, `schema-offload`, `schema-bibliotheque`, `schema-ci-cd`.

## 6. Ce que ça démontre (formulation entretien)
Sans chiffrement, MQTT livre les identifiants et les commandes à quiconque observe le réseau : la confidentialité d'un système IoT ne peut pas reposer sur le protocole applicatif, elle doit être assurée par la couche transport. Et un outil de défense est lui-même une cible : il doit traiter chaque paquet exactement comme la machine qu'il protège, sous peine d'être trompé (insertion, évasion), et ne jamais faire confiance à une longueur annoncée par le réseau.

## 7. Lien vers la remédiation IAM
La couche IAM du mois 5 neutralise directement la faiblesse exploitée ici :
- **mTLS** chiffre le canal MQTT (port 8883) : rejoué sur le même réseau, rnifler ne verrait plus que des enregistrements TLS, sans identifiant ni message lisible. Le sniffer devient ainsi la preuve de la remédiation, par une comparaison avant/après sur le même trafic.
- **L'authentification mutuelle par certificats**, émis par l'autorité de certification du lab, remplace les identifiants partagés et supprime les connexions anonymes : il n'y a plus de mot de passe à intercepter.
- **Les jetons de courte durée** limitent la fenêtre d'exploitation d'un secret qui serait malgré tout dérobé, par exemple par l'attaque de l'homme du milieu ; **la révocation** permet d'exclure immédiatement un objet compromis.

Conséquence pour la suite du lab : une fois le trafic chiffré, rnifler perd la vision du contenu des messages, et c'est le but recherché. L'IDS devra alors s'appuyer sur les métadonnées (connexions, cadence, volumes, échecs de négociation TLS) ou sur les journaux du broker, seul point où les messages sont déchiffrés.

## 8. Statut portfolio
- **Autonome ou intégré :** autonome — projet à part entière, conçu comme une brique indépendante et réutilisé par `sec-lab-iot`.
- **Lien si autonome :** dépôt https://github.com/MkDriss/rnifler ; page portfolio `my-projects/rnifler`.
