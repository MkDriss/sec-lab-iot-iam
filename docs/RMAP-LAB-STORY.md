# LAB-STORY — rmap (scanner de ports TCP)

> Fiche normalisée destinée à l'assemblage du rapport final du lab `sec-lab-iot`.
> Même structure pour tous les sous-projets. Ton factuel, phrases réutilisables
> telles quelles dans un rapport.

## 1. Nom & rôle (une phrase)
rmap est un scanner de ports TCP écrit de zéro en Rust, qui cartographie le réseau du lab : il repère les machines vivantes, retrouve leur nom et liste leurs ports ouverts, fermés ou filtrés.

## 2. Place dans le récit
- **Phase :** reconnaissance (premier outil offensif du lab).
- **Vient après :** la mise en place du réseau IoT segmenté (routeur OpenWrt, VLAN isolé) et le flashage des objets ESP32 volontairement vulnérables. rmap est le premier outil qui agit sur ce réseau.
- **Mène à :** le sniffer de paquets (qui réutilise des briques de rmap et donne l'accès bas niveau qui manque ici), puis l'attaque MITM sur MQTT, qui vise précisément le broker Mosquitto repéré par rmap sur le port 1883.

## 3. Vulnérabilité ciblée
- **Faille :** surface d'exposition. rmap ne casse rien : il révèle quels services sont joignables sur le réseau, information de départ de toute attaque. Dans le lab, il met en évidence des services ouverts sans authentification, comme le broker MQTT sur 1883.
- **Item OWASP IoT (si applicable) :** contribue au constat de I1 — Weak, Guessable, or Hardcoded Passwords et I2 — Insecure Network Services, en exposant les services réseau atteignables. rmap les rend visibles ; leur exploitation vient avec les outils suivants. *(À confirmer : vérifier la numérotation exacte sur la liste OWASP IoT Top 10 2018, voir note en fin de fiche.)*
- **Pilier CIA touché :** Confidentialité — la cartographie révèle des informations sur le réseau ; c'est le préalable aux atteintes à l'intégrité et à la disponibilité menées par les outils suivants.

## 4. Ce que j'ai fait
- **Approche :** construction incrémentale, validée à chaque étape. Scan séquentiel de base, puis passage à l'asynchrone concurrent, ajout d'un timeout par port, lecture des plages de ports et d'adresses façon nmap (CIDR inclus), découverte des machines vivantes, et résolution DNS inverse des noms. Chaque étape a été testée avant la suivante.
- **Choix techniques :** Rust, runtime asynchrone tokio (tâches concurrentes, sémaphore pour borner les connexions simultanées, timeout), crate clap pour la ligne de commande, crate dns-lookup pour la résolution inverse. Code séparé en bibliothèque réutilisable et interface en ligne de commande, couvert par des tests automatisés, vérifié par rustfmt et clippy.
- **Pourquoi ces choix :** recodé à la main plutôt qu'en utilisant nmap, pour comprendre ce qui se passe réellement sur le réseau (poignée de main TCP, interprétation d'un silence, coût d'un scan massif). Rust pour apprendre un langage système sûr et pour que les briques (scan concurrent, parsing) soient réutilisables par les outils suivants du lab. TCP connect sans privilèges, pour rester simple et sûr à ce stade.

## 5. Résultat & preuve
- **Ce qui s'est passé :** rmap scanne le /24 du lab, repère les machines vivantes et liste leurs ports. Exemple réel obtenu sur le lab : la machine `pipy.lan` (192.168.50.182) ressort avec les ports 22 (SSH) et 1883 (MQTT) ouverts — le broker Mosquitto visé par la suite. La découverte réduit un /24 de 256 adresses à la poignée de machines réellement présentes.
- **Fichiers média associés :**
  - `rmap-discover-lab.png` — sortie d'un scan avec `--discover` sur le /24, liste des machines vivantes avec leurs noms + rapport par machine (la vraie preuve sur le lab).
  - `rmap-reliability-before.png` et `rmap-reliability-after.png` — captures avant/après du bug des faux négatifs silencieux (voir rubrique 6).
  - `rmap-port-states.png` — schéma des trois réactions d'un port (ouvert / fermé / filtré), déjà réalisé.
  - *(mesures de temps à capturer : scan direct vs `--discover` sur le /24, avec `time`, pour chiffrer le gain.)*

## 6. Ce que ça démontre (formulation entretien)
Le concept central n'est pas le scan lui-même, mais la fiabilité d'un outil de sécurité. En limitant volontairement les ressources du système lors d'un test, j'ai découvert que le scanner déclarait « fermés » des ports en réalité ouverts, sans le moindre avertissement : un faux négatif silencieux. Pour un outil de sécurité, c'est le pire défaut, car il donne une fausse impression de sécurité. Je l'ai corrigé en trois temps : reproduire le problème par un test, le rendre visible avec un état « erreur » distinct et un avertissement explicite, puis le corriger en rendant la charge réglable. La leçon, réutilisable face à un recruteur : un outil de sécurité doit signaler ce qu'il n'a pas pu vérifier, car un résultat incomplet présenté comme complet est plus dangereux qu'une absence de résultat.

Deuxième idée démontrable : la reconnaissance est le préalable de toute sécurité. On ne protège que ce qu'on sait exposé, et rmap est l'outil qui établit cette carte.

## 7. Lien vers la remédiation IAM
La couche IAM du mois 5 répond directement à ce que rmap met en évidence. rmap révèle des services réseau joignables sans authentification (le MQTT sur 1883 en est l'exemple) : la remédiation consiste à placer ces services derrière une authentification mutuelle TLS (mTLS), de sorte qu'ouvrir une connexion TCP ne suffise plus — il faut présenter un certificat client valide. Un scan comme rmap verra toujours le port ouvert, mais la connexion sera refusée sans identité valide, et les jetons courts et la révocation limitent la fenêtre d'usage d'un accès compromis. Le bouclage du récit : rmap montre la surface exposée au début du lab, l'IAM referme cette surface à la fin. Mieux, rmap sert alors d'outil de contrôle, en vérifiant que les services autrefois ouverts ne répondent plus qu'à une identité authentifiée.

## 8. Statut portfolio
- **Autonome ou intégré :** les deux. Brique intégrée au récit de `sec-lab-iot`, et projet autonome avec sa propre page de portfolio et son dépôt dédié (code recodé à la main, crédible seul).
- **Lien si autonome :** dépôt GitHub `https://github.com/MkDriss/rmap` ; page de portfolio dédiée (textes dans `rmap.texts.ts`).

---

### Notes à vérifier avant intégration au rapport final
1. **OWASP IoT (rubrique 3).** J'ai rattaché rmap à I1 et I2 de l'OWASP IoT Top 10 (version 2018), mais rmap *révèle* ces faiblesses sans les exploiter : à toi de voir si tu préfères ne citer OWASP que pour les outils qui exploitent réellement (MITM, fuzzer), et laisser « N/A — outil de reconnaissance » ici. La numérotation I1/I2 est à confirmer sur la liste officielle.
2. **Média du lab (rubrique 5).** `pipy.lan` et les ports 22/1883 viennent de ta vraie sortie de scan. Les noms de fichiers média sont des propositions : adapte-les à ta convention de nommage.
3. **Chiffres de performance.** Le gain de la découverte n'est pas encore chiffré : une mesure `time` avant/après sur ton /24 rendrait la rubrique 5 plus concrète.
