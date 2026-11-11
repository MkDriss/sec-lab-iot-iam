# Entrée sur le réseau — Capture & crack d'un handshake WPA2

> **Sous-projet de [`sec-lab-iot-iam`](../).** Il s'agit de l'étape « entrée sur le
> réseau » : l'accès initial qui précède et conditionne les attaques menées ensuite
> sur les objets connectés du labo. Documenté comme une *kill chain* : accès réseau →
> reconnaissance → exploitation des objets.

---

## ⚖️ 1. Cadre légal et périmètre

**Cette manipulation vise exclusivement mon propre réseau WiFi, dans mon laboratoire
isolé, sur du matériel que je possède.** C'est légal parce que c'est mon réseau.

| Élément | Détail |
|---|---|
| Réseau cible | `IoT-Lab` — mon point d'accès de labo |
| Point d'accès | GL.iNet **GL-MT300N-V2** (« Mango »), que je possède |
| Client de test | un **ESP32** sous mon contrôle |
| Poste d'attaque | ThinkPad X390 sous Fedora Linux |
| Environnement | labo isolé, aucun tiers impacté |

> ⚠️ Attaquer un réseau WiFi dont on n'est pas propriétaire (ou sans autorisation
> écrite explicite) est illégal dans la plupart des juridictions. Ce document décrit
> un exercice réalisé **uniquement** sur mon propre matériel.

---

## 🎯 2. Objectif et scénario

Reconstituer la chaîne complète « entrée sur le réseau » avec des **outils standard
et reconnus** (suite `aircrack-ng`) — l'objectif est de comprendre et documenter
l'attaque, pas de ré-implémenter les outils.

La *kill chain* en 5 étapes :

1. **Mode monitor** — mettre la carte WiFi en écoute passive des trames 802.11.
2. **Reconnaissance** — repérer `IoT-Lab` (2,4 GHz) et son canal.
3. **Capture du handshake** — récupérer le 4-way handshake WPA2. Particularité de ce
   labo : je contrôle le client, donc je **redémarre l'ESP32** pour forcer sa
   reconnexion et générer le handshake — pas besoin d'attaque de désauthentification
   ni d'injection de paquets.
4. **Crack hors ligne** — retrouver la passphrase avec `aircrack-ng` + dictionnaire.
5. **Connexion** — rejoindre `IoT-Lab` avec le mot de passe retrouvé.

---

## 🔧 3. Test de faisabilité matériel

Avant toute attaque, vérifier que la carte WiFi intégrée du ThinkPad est capable de
faire du **mode monitor**. Ce n'est pas une évidence : la carte est une **Intel
Wireless-AC 9560** pilotée par le driver `iwlwifi`, et cette carte précise a une
réputation mitigée sous Linux.

### Ce que dit la littérature

- De façon générale, le driver `iwlwifi` **supporte** le mode monitor et l'injection.
- **Mais pour la 9560 spécifiquement**, une étude du **Fraunhofer AISEC** rapporte que
  le mode monitor donne des **résultats incohérents** sur cette carte.
- L'écosystème (projet `hcxdumptool`) classe d'ailleurs Intel parmi les chipsets
  **non recommandés** pour ce type d'usage (« monitor mode and frame injection
  problems »), en rappelant qu'**annoncer** le mode monitor n'implique pas
  **savoir capturer** réellement.

> **Conséquence méthodologique :** je ne me fie pas à la liste des modes annoncés.
> Je teste **empiriquement** que la carte capture vraiment. L'injection, elle, m'est
> inutile (je reboote l'ESP32 manuellement), ce qui neutralise la principale faiblesse
> d'Intel.

### 3.1 — Identifier la carte et le driver

```bash
lspci -k | grep -A3 -i network
```

```
00:14.3 Network controller: Intel Corporation Cannon Point-LP CNVi [Wireless-AC] (rev 30)
    Kernel driver in use: iwlwifi
    Kernel modules: iwlwifi
```

Interface logique récupérée via `iw dev` : **`wlp0s20f3`**.

### 3.2 — Vérifier que le mode monitor est supporté

```bash
iw list | grep -A 10 "Supported interface modes"
```

![Modes d'interface supportés — « monitor » est présent](interface-modes.png)

➡️ `* monitor` figure dans la liste : **condition nécessaire remplie** (mais pas
suffisante sur cette carte — d'où le test réel ci-dessous).

### 3.3 — Bascule réelle en mode monitor + vérification

> ⚠️ Cette étape **coupe la connexion WiFi** du poste : `airmon-ng check kill` arrête
> `NetworkManager` et `wpa_supplicant`.

```bash
sudo airmon-ng check        # liste les processus gênants
sudo airmon-ng check kill   # les arrête (coupe le WiFi)
sudo airmon-ng start wlp0s20f3
iw dev                      # doit afficher "type monitor"
```

![Bascule en mode monitor réussie + identification du chipset](monitor-check_setup.png)

➡️ `iw dev` affiche `type monitor` sur l'interface `wlp0s20f3mon` : la bascule est
réelle. La sortie d'`airmon-ng start` confirme aussi le chipset
(`iwlwifi` / Intel Cannon Point-LP CNVi [Wireless-AC]).

> *Note :* `airmon-ng` affiche au passage une ligne `non-mac80211 device? (report this!)`
> pour le pseudo-périphérique P2P de la carte. C'est un **artefact cosmétique** sans
> impact sur la capture.

**Retour à l'état initial** (après chaque session monitor) :

```bash
sudo airmon-ng stop wlp0s20f3mon
sudo systemctl start NetworkManager
```

---

## 📡 4. Reconnaissance

Carte en monitor, on scanne les réseaux environnants pour localiser la cible. Ce scan
sert aussi de **juge de paix** : si des réseaux remontent et se mettent à jour, c'est
que la 9560 **capture réellement** (le piège des cartes « incohérentes » étant de
basculer en monitor sans rien capturer).

```bash
sudo airodump-ng wlp0s20f3mon
```

![Scan airodump-ng — réseau IoT-Lab surligné](iot-lab-scan-highlight.png)

➡️ La carte capture bien : des dizaines de réseaux remontent, le compteur de beacons
progresse. **Test de faisabilité concluant** — ce projet tient sur la carte intégrée,
sans adaptateur USB externe.

**Cible identifiée :**

| Champ | Valeur |
|---|---|
| ESSID | `IoT-Lab` |
| BSSID | `96:83:C4:96:A7:13` |
| Canal | `1` (2,4 GHz) |
| Sécurité | **WPA2 / CCMP / PSK** |
| Signal (PWR) | ≈ −16 dBm (AP très proche, cohérent avec le labo) |

> Le point d'accès `GL-MT300N-V2-713` apparaît à la même puissance dans le scan : c'est
> le nom par défaut de mon routeur de labo, ce qui confirme que **c'est bien mon
> matériel** qui héberge `IoT-Lab`.

> 🔒 *Hygiène :* sur la version publiée, les BSSID/ESSID des réseaux voisins sont
> floutés. Seule ma cible `IoT-Lab` est laissée visible.

---

## 🤝 5. Capture du handshake WPA2

Le scan de découverte tourne en *channel-hopping* — inadapté pour capturer un
handshake. On **verrouille** donc airodump sur le canal et le BSSID de la cible, et on
**écrit** la capture dans un fichier :

```bash
sudo airodump-ng -c 1 --bssid 96:83:C4:96:A7:13 -w iotlab-handshake wlp0s20f3mon
```

On laisse tourner, puis on **redémarre l'ESP32** : il se reconnecte à `IoT-Lab` et
rejoue le 4-way handshake. C'est la parade « reboot au lieu de déauth » — aucune
injection requise.

![Capture live — EAPOL visible dans la colonne Notes](handshake-live-eapol.png)
<!-- ⚠️ renomme ta capture #5 (airodump live) en handshake-live-eapol.png, ou ajuste ce lien -->

➡️ En face de la station **`70:4B:CA:03:7D:74`** (l'ESP32), la colonne **Notes**
affiche **`EAPOL`** : des trames du handshake ont été captées.

### 5.1 — Vérifier que le handshake est exploitable

**Voir `EAPOL` ne garantit pas un handshake complet.** On vérifie le fichier :

```bash
aircrack-ng iotlab-handshake-01.cap
```

```
   #  BSSID              ESSID      Encryption
   1  96:83:C4:96:A7:13  IoT-Lab    WPA (1 handshake)
```

➡️ `WPA (1 handshake)` : le handshake capturé est **complet et exploitable**.

### 5.2 — Contrôle rigoureux dans Wireshark

Pour confirmer visuellement les 4 messages du 4-way handshake :

```bash
wireshark iotlab-handshake-01.cap   # filtre d'affichage : eapol
```

![Wireshark — les 4 messages EAPOL du handshake](wireshark-eapol-4messages.png)
<!-- ⚠️ renomme ta capture #7 (Wireshark) en wireshark-eapol-4messages.png, ou ajuste ce lien -->

➡️ Les trames étiquetées « Message 1 of 4 » à « Message 4 of 4 » confirment un
handshake complet.

---

## 🔑 6. Crack hors ligne

Le handshake capturé contient les nonces et un **MIC** (signature dérivée du mot de
passe). Cela suffit à **tester des candidats hors ligne**, sans jamais reparler au
routeur : pour chaque mot du dictionnaire, `aircrack-ng` dérive la clé, recalcule le
MIC et le compare à celui capturé. C'est **ce modèle hors ligne qui rend WPA2-PSK
vulnérable** (voir §8).

```bash
aircrack-ng iotlab-handshake-01.cap -w wordlist.txt
```

```
                           KEY FOUND! [ LabIoT2026 ]

   Master Key : 0D 18 31 2B F1 12 C8 80 B2 B2 2C 43 8C 86 2D 5A ...
   EAPOL HMAC : D6 B6 4F EC FB 2D DB 47 07 19 3D 2C D3 5E 45 DB
```

![aircrack-ng — KEY FOUND](key-found.png)
<!-- ⚠️ renomme ta capture #8 (KEY FOUND) en key-found.png, ou ajuste ce lien -->

➡️ Mot de passe retrouvé : **`LabIoT2026`**.

### ⚠️ Cadrage méthodologique honnête

**`LabIoT2026` n'était pas présent nativement dans `rockyou.txt`.** Je l'ai ajouté au
dictionnaire pour démontrer la chaîne complète *capture → crack* de bout en bout.

Ce choix est assumé et important à préciser : il montre que l'attaque par dictionnaire
**réussit dès lors que le mot de passe figure dans la liste**. La vraie question n'est
donc pas « rockyou contenait-il mon mot de passe ? » mais :

> **Mon mot de passe aurait-il pu s'y trouver ?**

Et la réponse est **oui, très probablement**. `LabIoT2026` suit le motif
`Mot + Année` (majuscule, minuscules, millésime, 10 caractères) — exactement le type
de candidat que génèrent les dictionnaires dérivés (`rockyou` + règles de mutation
type `best64`). La leçon n'est pas « rockyou contenait ce mot de passe » mais **« un
mot de passe de ce format tombe face à une attaque par dictionnaire + règles »**.

> 💡 La capture `KEY FOUND!` prouve donc le **mécanisme** (le crack hors ligne
> fonctionne), pas que ce mot de passe spécifique figurait dans une fuite publique.

---

## 🌐 7. Connexion avec le mot de passe cracké

Retour en mode managed, puis connexion à `IoT-Lab` :

```bash
sudo airmon-ng stop wlp0s20f3mon
sudo systemctl start NetworkManager
```

### Piège rencontré : bug `nmcli` ≥ 1.50

La forme « simple » a échoué :

```bash
$ nmcli device wifi connect "IoT-Lab" password "LabIoT2026"
Error: 802-11-wireless-security.key-mgmt: property is missing.
```

C'est un **comportement connu de `nmcli` à partir de la version ~1.50**, qui touche
surtout les réseaux dont NetworkManager a déjà un profil enregistré (ce qui était mon
cas après une première connexion). Deux contournements :

**Contournement rapide** (interactif) :

```bash
nmcli device wifi connect "IoT-Lab" --ask
```

**Procédure de référence** (non interactive, explicite — recommandée en doc) :

```bash
nmcli connection add type wifi con-name "IoT-Lab" ifname wlp0s20f3 ssid "IoT-Lab" \
  wifi-sec.key-mgmt wpa-psk wifi-sec.psk "LabIoT2026"
nmcli connection up "IoT-Lab"
```

Ici, on renseigne explicitement `wifi-sec.key-mgmt wpa-psk`, ce qui est précisément le
champ que la forme « simple » omet (d'où le `property is missing`).

![Connexion réussie à IoT-Lab + adresse IP obtenue](nmcli-connected.png)
<!-- ⚠️ renomme ta capture #9 (connexion + IP) en nmcli-connected.png, ou ajuste ce lien -->

➡️ Connexion active et IP obtenue sur `IoT-Lab`. **La boucle est bouclée** : parti de
rien, je suis désormais sur le réseau, uniquement par capture + crack hors ligne.

---

## 🛡️ 8. Leçon défensive : pourquoi ça marche, et comment s'en protéger

### Pourquoi l'attaque réussit

Elle repose sur **deux propriétés combinées** de WPA2-PSK :

1. **Le crack est hors ligne.** Le 4-way handshake capturé est un *oracle* : il permet
   de vérifier des mots de passe **sur ma machine**, à la vitesse du CPU/GPU, sans
   limite et sans que le routeur ne s'en aperçoive.
2. **Le mot de passe est dans l'espace de recherche d'un dictionnaire.** `LabIoT2026`
   suit un motif prévisible que `rockyou` + règles génèrent.

Retirez l'une des deux, l'attaque échoue.

### Défense 1 (idéale) : WPA3-SAE

WPA3-Personal remplace le 4-way handshake par **SAE** (*Simultaneous Authentication of
Equals*, alias **Dragonfly**). Propriété clé : SAE est **conçu pour résister aux
attaques par dictionnaire hors ligne**. Même en capturant tout l'échange, un attaquant
**ne peut pas en extraire d'oracle hors ligne** — il est repoussé vers une attaque
**en ligne**, un essai à la fois contre le routeur : lente, bruyante, et bloquable.

| | WPA2-PSK | WPA3-SAE |
|---|---|---|
| Capturer l'échange d'authentification | ✅ | ✅ |
| En extraire un vérificateur de mot de passe | ✅ (le MIC) | ❌ |
| Tester des millions de candidats **hors ligne** | ✅ (vitesse GPU) | ❌ |
| Attaque restante | — | **en ligne** uniquement (lente, détectable) |

**Nuances honnêtes** (WPA3 n'est pas « incassable ») :

- La famille d'attaques **Dragonblood** (Vanhoef & Ronen, 2019) a révélé des fuites par
  canaux auxiliaires (timing / cache) dans l'encodage du mot de passe de SAE, ainsi que
  des attaques de *downgrade* en **mode transition** WPA2/WPA3. Ce sont des failles
  d'**implémentation** (largement corrigées depuis), mais elles rappellent que WPA3 ≠
  invulnérable.
- Un mot de passe faible reste faible : WPA3 bloque le crack **hors ligne**, pas une
  attaque en ligne contre `12345678`.
- Le bénéfice réel suppose du **WPA3 seul** (le mode mixte WPA2/WPA3 peut être
  *downgradé*).

### Défense 2 (réaliste et immédiate) : passphrase longue et aléatoire

Mon **GL-MT300N-V2** repose sur un SoC **MediaTek MT7628NN**, limité au **802.11 b/g/n
en 2,4 GHz** (plateforme de 2017, 16 Mo de flash). En pratique, **le WPA3 n'y est pas
disponible simplement** (il faudrait recompiler/installer le bon paquet `wpad` avec
SAE, ce que la place en flash rend hasardeux). La recommandation « cible » WPA3 n'est
donc **pas applicable aisément sur ce matériel**.

La mesure **immédiatement applicable** est donc d'agir sur le **second** facteur : une
**passphrase longue et véritablement aléatoire** (p. ex. 5–6 mots aléatoires, ou 20+
caractères sans motif). Elle met le mot de passe **hors de portée** de toute attaque
par dictionnaire + règles — y compris le crack hors ligne de WPA2 que je viens de
démontrer. `LabIoT2026` échoue à ce test ; `correct-horse-battery-staple-7fz`, non.

### Synthèse

| Facteur exploité | Défense | Applicable sur ce labo ? |
|---|---|---|
| Crack **hors ligne** du handshake | Migrer vers **WPA3-SAE** | ❌ Pas aisément (matériel 2017) |
| Mot de passe **devinable** | **Passphrase longue aléatoire** | ✅ Immédiat |

> Cette étape valide l'**accès réseau**. Les attaques suivantes de `sec-lab-iot-iam`
> partent de cette position : une fois sur `IoT-Lab`, je peux atteindre et
> interroger les objets connectés du labo.

---

## 📚 9. Bibliographie

Sources techniques citées (consultées en novembre 2026) :

| Sujet | Source |
|---|---|
| Suite aircrack-ng (doc officielle) | <https://www.aircrack-ng.org/documentation.html> |
| Liste de compatibilité des drivers | <https://www.aircrack-ng.org/doku.php?id=compatibility_drivers> |
| Driver `iwlwifi` (wiki kernel Linux) | <https://wireless.wiki.kernel.org/en/users/drivers/iwlwifi> |
| Outil `iw` (wiki kernel Linux) | <https://wireless.wiki.kernel.org/en/users/documentation/iw> |
| Monitor mode / injection sur 9560 (Fraunhofer AISEC) | <https://www.cybersecurity.blog.aisec.fraunhofer.de/?p=2285> |
| Spécifications GL-MT300N-V2 (doc constructeur) | <https://docs.gl-inet.com/router/en/3/specification/gl-mt300n-v2/> |
| Dragonblood — analyse de SAE (Vanhoef & Ronen, 2019) | <https://dl.aircrack-ng.org/wiki-files/doc/additional_papers/dragonblood.pdf> |

> Les commandes `airmon-ng`, `airodump-ng`, `aircrack-ng`, `iw` et `nmcli` sont
> également documentées dans leurs *man pages* respectives (`man <outil>`).

---

## 🧰 Annexe — Matériel & versions

| Composant | Détail |
|---|---|
| Poste d'attaque | ThinkPad X390, Fedora Linux (x86-64) |
| Carte WiFi | Intel Wireless-AC 9560 (Cannon Point-LP CNVi), driver `iwlwifi` |
| Suite d'outils | `aircrack-ng` 1.7 |
| Point d'accès cible | GL.iNet GL-MT300N-V2 (MediaTek MT7628NN, 802.11n 2,4 GHz) |
| Client de test | ESP32 |

---

*Projet pédagogique réalisé sur mon propre matériel, en environnement isolé, à des fins
d'apprentissage de la sécurité WiFi et de documentation défensive.*
