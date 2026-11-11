# Extraction de firmware ESP32 & configuration MQTT (`actionneur1`)

> Volet d'appoint du labo **`sec-lab-iot`**. Objectif borné : extraire le firmware d'un
> ESP32 et y récupérer **l'identité et la configuration MQTT de l'objet**, « comme un
> attaquant ». Illustre deux items du modèle de menaces OWASP IoT — **I10 — Lack of
> Physical Hardening** et **I2 — Insecure Network Services** — et débouche sur une
> **usurpation d'identité MQTT** rendue possible par un broker en accès anonyme.

## Contexte & périmètre

Volet volontairement court (~5 % du projet). Preuve ciblée : montrer qu'un accès
physique à la puce permet de reconstituer, par simple analyse statique, tout ce qu'il
faut pour usurper un objet du labo sur son broker MQTT.

- **Cible** : `actionneur1`, un ESP32 DevKit **WROOM-32** (puce Xtensa LX6), client
  MQTT se connectant au broker **Mosquitto** du labo (hébergé sur le Raspberry Pi),
  actuellement configuré en **accès anonyme**.
- **Firmware** : écrit maison en C++/Arduino (PlatformIO).
- **Résultat attendu** : récupérer le `clientId` embarqué — et, au-delà, toute la
  config de connexion.

## Environnement

| Élément | Détail |
|---|---|
| Hôte | Fedora Linux (x86_64), utilisateur dans le groupe `dialout` |
| Liaison | ESP32 en USB, vu sous `/dev/ttyUSB0` |
| Extraction | `esptool` |
| Analyse | `strings`, puis Ghidra **12.2** (JDK 21) |
| Reconstruction ELF | `esp32_image_parser` (Tenable) + fork `samyk` |

Note : le support natif de l'architecture Xtensa est intégré à Ghidra depuis la
version **11.0**, donc aucun module tiers n'est nécessaire sur une 12.x.

## Démarche

### 1. Dump de la flash via `esptool` — illustre I10

La flash complète (4 Mo) est lue par le port série, via le bootloader série de la
puce :

```bash
esptool.py -p /dev/ttyUSB0 -b 460800 read_flash 0 ALL flash_dump.bin
```

- `read_flash 0 ALL` : lecture depuis l'offset 0, taille autodétectée.
- `-b 460800` : débit du transfert (la connexion initiale reste à 115200).

**Lecture sécurité (I10)** : l'intégralité de la flash a pu être extraite via
l'interface série, sans *flash encryption* ni *secure boot*. L'absence de durcissement
de cette interface est précisément ce que couvre l'item I10.

📸 `dumping.png` — le `read_flash` en cours dans le terminal.

### 2. Première passe : `strings` — récupération du `clientId`

Avant tout reverse, le réflexe trivial : chercher les chaînes lisibles dans le dump.

```bash
strings flash_dump.bin | grep -i -E 'client|mqtt|actionneur'
# -> actionneur1
```

Le `clientId` apparaît **en clair**. Contrairement à un secret planté et jamais
employé, un `clientId` est une **donnée opérationnelle** : il est effectivement utilisé
par le firmware pour s'identifier auprès du broker.

📸 (optionnel) `clientid-strings.png` — le `clientId` en clair dans la sortie de `strings`.

### 3. Analyse statique approfondie : reconstruction d'un ELF

Un dump brut n'a pas de carte mémoire : Ghidra ne sait pas à quelle adresse virtuelle
chaque octet « vit », ce qui empêche le calcul correct des références croisées. On
convertit donc la partition applicative en ELF avec `esp32_image_parser`.

Inventaire des partitions :

```bash
python3 esp32_image_parser.py show_partitions flash_dump.bin
```

| Partition | Offset | Type |
|---|---|---|
| nvs | 0x9000 | données (NVS) |
| otadata | 0xe000 | données (OTA) |
| **app0** | **0x10000** | **application (ota_0)** |
| app1 | 0x150000 | application (ota_1) |
| spiffs | 0x290000 | données (FS) |
| coredump | 0x3f0000 | données |

Reconstruction de l'ELF depuis `app0` :

```bash
python3 esp32_image_parser.py create_elf flash_dump.bin -partition app0 -output firmware.elf
```

#### Friction outillage (documentée)

`esp32_image_parser` n'est plus activement maintenu et a nécessité **trois correctifs**
pour traiter cette image. Ce diagnostic fait partie du livrable : il montre la capacité
à lire, comprendre et patcher un outil upstream cassé.

1. **`NameError: LoadFirmwareImage`** — le script importe `from esptool import *`, mais
   dans les esptool récents cette fonction a migré vers `esptool.bin_image`. Correctif :
   ajouter `from esptool.bin_image import *` (cf. *issue #4* et *PR #11*).
2. **`KeyError: '.dram0.data'`** — segment DRAM « non-DMA » non reconnu par la table de
   mapping. Résolu en passant au **fork `samyk`** (*PR #9 — Support non-DMA DRAM & fix
   create_elf bugs*).
3. **`KeyError: '.iram0.text'`** — la routine d'écriture des *program headers* réclame
   des sections IRAM absentes de cette image. Correctif manuel : filtrer la liste des
   sections pour ne garder que celles réellement reconstruites (*issue #14*, *PR #15 —
   Fix iram0 combine*).

L'ELF final contient `.flash.text` (code / IROM), `.flash.rodata` (constantes / DROM)
et `.dram0.data`. Les sections IRAM et RTC sont absentes, **sans incidence** : les
constantes recherchées sont en `.flash.rodata` et le code qui les emploie est en
`.flash.text`.

### 4. Localisation, usage et reconstitution de la config dans Ghidra

Import de `firmware.elf` (architecture Xtensa détectée via l'en-tête ELF), puis
auto-analyse.

**La valeur.** `Search → Memory` (*Format : String*) retrouve le `clientId`
**`actionneur1` à `0x3f400138`**, dans `.flash.rodata`. La chaîne, une fois définie
(`ds "actionneur1"`), porte **`XREF[9]`** : elle est réellement référencée par le code,
contrairement à une donnée morte.

📸 `ghidra-clientId-found.png` — le `clientId` dans `.flash.rodata`, avec ses XREF.

**L'usage.** En suivant une de ces références, on tombe sur la routine de connexion
MQTT (vue *Listing*). Le `clientId` y est chargé puis passé à l'appel de connexion
(`call8`), ce que confirme la chaîne de log voisine `[MQTT] Connexion a %s:%u (client …)`.

📸 `ghidra-clientId-usage.png` — le `clientId` chargé et utilisé dans le code de connexion.

**La config complète, en prime.** Le contexte immédiat de cette routine expose toute la
configuration de connexion, codée en dur :

- **Broker** : chaîne `192.168.50.182` (référencée par `l32r` juste avant l'appel).
- **Port** : `movi a13,0x75b` → `0x75b` = **1883** (port MQTT par défaut).
- **Topics** : en `.flash.rodata`, `labo/actionneur/commande` (`0x3f400144`) et
  `labo/actionneur/etat` (`0x3f400160`), chacun avec ses XREF.

**Limite d'outillage notée** : le support Xtensa de Ghidra couvre bien le
désassemblage, mais le **décompilateur** est immature pour cette architecture
(instructions non implémentées, convention d'appel à fenêtres de registres mal gérée).
L'analyse s'appuie donc sur la vue *Listing*, pas sur le pseudo-C.

## Résultat

Par simple analyse statique, toute la configuration MQTT de l'objet a été reconstituée :

| Élément | Valeur extraite | Source |
|---|---|---|
| Broker | `192.168.50.182` | `.flash.rodata` |
| Port | `1883` (`0x75b`) | immédiat dans le code de connexion |
| clientId | `actionneur1` | `0x3f400138`, `.flash.rodata` (XREF[9]) |
| Topic commande | `labo/actionneur/commande` | `0x3f400144`, `.flash.rodata` |
| Topic état | `labo/actionneur/etat` | `0x3f400160`, `.flash.rodata` |

Récupération confirmée par deux voies indépendantes : `strings` sur le dump brut, puis
analyse statique dans Ghidra (valeurs localisées et confirmées comme réellement
référencées par le code).

## Impact — usurpation d'identité MQTT (kit complet)

Le broker Mosquitto est en **accès anonyme** : aucun identifiant n'est requis. Les
éléments extraits forment donc un **kit d'usurpation complet** — où se connecter, sous
quelle identité, et quels topics manipuler. Dans le rôle de l'objet (il s'abonne à
`…/commande` et publie sur `…/etat`), un attaquant peut :

- **Évincer l'objet légitime (Disponibilité)** — si un `clientId` correspond à un client
  déjà connecté, le broker *doit* déconnecter le client existant (MQTT v3.1.1,
  *[MQTT-3.1.4-3]*). Se connecter avec `actionneur1` fait tomber l'actionneur hors ligne.
- **Intercepter les commandes** — en s'abonnant à `labo/actionneur/commande` sous cette
  identité, l'attaquant reçoit les ordres destinés à l'objet (qui, évincé, ne les
  exécute plus).
- **Injecter de la fausse télémétrie (Intégrité)** — en publiant sur
  `labo/actionneur/etat`, il trompe le contrôleur / le tableau de bord sur l'état réel.

Items OWASP IoT démontrés : **I10** (flash extractible faute de durcissement) et
**I2 — Insecure Network Services** (le broker accepte, en anonyme, une identité copiable
et forgeable).

> *Note de cadrage* : l'item **I1 (Hardcoded Passwords)** du modèle de menaces initial
> ne s'applique pas à ce chemin — un `clientId` est un **identifiant**, pas un secret
> d'authentification, et le broker étant anonyme, aucun mot de passe n'est en jeu. Le
> cœur de la faille est l'**identité d'appareil** faible et extractible, pas un secret.

## Remédiation (prévue — mois 5)

- **Identité d'appareil** : remplacement de l'identité fondée sur une chaîne copiable
  par une **identité machine par certificat (mTLS)**, propre à chaque objet et
  **révocable**. Copier le firmware ne suffit plus à usurper l'objet : il faudrait sa
  clé privée.
- **Broker** : passage de l'anonyme à une authentification mutuelle TLS (certificat
  client exigé). Le `clientId` cesse alors d'être une frontière de sécurité, et une
  politique d'ACL peut restreindre chaque identité à ses propres topics.
- **I10 (pistes)** : activation du *flash encryption* et du *secure boot* de l'ESP32
  pour empêcher une extraction aussi directe.

## Notes d'ingénierie

- **Les XREF comme filtre** : distinguer les chaînes *opérationnelles* (le `clientId`,
  9 références) des données inertes permet de cibler directement ce qui compte dans un
  binaire — et, de proche en proche, de reconstituer toute la config de connexion.
- **Savoir s'arrêter** : objectif atteint dès l'étape `strings` ; l'analyse Ghidra a
  confirmé, contextualisé et élargi le résultat, sans sur-ingénierie.
- **Friction assumée** : l'outil de référence open-source était à l'abandon ; les trois
  correctifs appliqués sont rattachés à leurs issues/PR amont.

## Références

- esptool — *Basic Commands / read_flash* (Espressif) :
  https://docs.espressif.com/projects/esptool/en/release-v4/esp32/esptool/basic-commands.html
- Ghidra (NSA) : https://github.com/NationalSecurityAgency/ghidra
- Support Xtensa natif depuis Ghidra 11.0 (note du module tiers) :
  https://github.com/Ebiroll/ghidra-xtensa
- `esp32_image_parser` (Tenable) : https://github.com/tenable/esp32_image_parser
- Fork `samyk` (correctifs create_elf) : https://github.com/samyk/esp32_image_parser
- Issues / PR citées (import, DRAM non-DMA, iram0) :
  https://github.com/tenable/esp32_image_parser/issues
- Limites du décompilateur Xtensa dans Ghidra (retour d'expérience Apriorit) :
  https://apriorit.com/dev-blog/reverse-reverse-engineer-iot-firmware
- Comportement `clientId` MQTT (unicité, éviction) — MQTT v3.1.1, *[MQTT-3.1.4-3]* :
  https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html

---

*Volet intégré au projet `sec-lab-iot` — labo de cybersécurité IoT/IAM isolé.
Usage offensif strictement limité au matériel de l'auteur.*
