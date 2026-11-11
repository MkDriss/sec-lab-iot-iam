# Capteur DHT22 : publication MQTT

> ⚠️ **Objet volontairement vulnérable** : les mesures sont publiées en clair, sans authentification.
> Il sert de cible pédagogique et ne doit être utilisé **que sur le réseau isolé `IoT-Lab`**.

## Objectif

Faire publier par un ESP32 équipé d'un capteur DHT22 sa température et son humidité, toutes les
5 secondes, vers le broker Mosquitto du Raspberry Pi.

## Prérequis

| Élément | Valeur dans ce labo |
|---|---|
| Carte | ESP32 DevKit WROOM-32 |
| Capteur | DHT22, broche OUT sur **GPIO 4** |
| Environnement | VS Code + PlatformIO (framework Arduino), sous Fedora |
| Broker | Mosquitto configuré sans authentification (voir `02-mosquitto-setup.md`), IP `192.168.50.182` |
| Réseau | WiFi `IoT-Lab` |

---

## 1. Contrat MQTT de l'objet

| Topic | Contenu (payload) | Unité | Exemple |
|---|---|---|---|
| `labo/capteur1/temperature` | Nombre en texte, 1 décimale | °C | `21.5` |
| `labo/capteur1/humidite` | Nombre en texte, 1 décimale | % HR | `48.2` |

| Paramètre | Valeur | Justification |
|---|---|---|
| Fréquence | 1 mesure / 5 s | Le DHT22 nécessite au moins 2 s entre deux lectures |
| QoS | 0 | PubSubClient ne publie qu'en QoS 0 |
| Message retenu | Non | Une mesure n'a de valeur qu'au moment où elle est prise |
| Identifiant client | `capteur1` | Doit être **unique** sur le broker (voir section 4) |
| Authentification | Aucune | Vulnérabilité étudiée |

Le format « un topic par mesure, valeur en texte » a été choisi pour sa lisibilité : le trafic
capturé sur le réseau est directement compréhensible.

## 2. Dépendances (`platformio.ini`)

Ajouter PubSubClient aux bibliothèques existantes (la bibliothèque DHT est déjà présente) :

```ini
lib_deps =
    ; ... bibliothèques DHT existantes ...
    knolleary/PubSubClient@^2.8
```

## 3. Gestion des secrets

Le mot de passe WiFi et l'adresse du broker ne sont **jamais** écrits dans le code source :

```
include/secrets.example.h   -> modèle, versionné (aucune valeur sensible)
include/secrets.h           -> vraies valeurs, ignoré par git (.gitignore)
```

```sh
cp include/secrets.example.h include/secrets.h
# éditer include/secrets.h : WIFI_SSID, WIFI_PASSWORD, MQTT_HOST
git status   # secrets.h ne doit PAS apparaître
```

> Un secret commité reste dans l'historique git même après suppression du fichier :
> il faut l'exclure **avant** le premier commit.

## 4. Points de conception importants

**Boucle non bloquante.** `loop()` ne contient aucun `delay()` long : le temps est mesuré avec
`millis()`. La documentation de PubSubClient précise que `loop()` doit être appelée régulièrement
pour traiter les messages entrants et maintenir la connexion au serveur.

**Reconnexion automatique.** Le WiFi et la session MQTT sont vérifiés à chaque tour de boucle.
En cas de coupure, une nouvelle tentative a lieu toutes les 5 s, sans bloquer le programme.

**Veille WiFi désactivée** (`WiFi.setSleep(false)`). Sur le routeur du labo, la mise en veille
WiFi provoquait des pertes de trames entrantes (problème diagnostiqué sur le Raspberry Pi :
45 % de pertes avec la veille active, 0 % sans). Le moniteur série affiche l'état de la veille
au démarrage pour vérification.

**Identifiant client unique.** La spécification MQTT 3.1.1 impose que, si un client se connecte
avec un identifiant déjà utilisé, le serveur déconnecte le client existant (règle MQTT-3.1.4-2).
Deux objets avec le même identifiant s'éjecteraient mutuellement en boucle. Ce comportement est
aussi exploitable : un attaquant connaissant l'identifiant peut déconnecter l'objet légitime.

**Lectures invalides ignorées.** Le DHT22 échoue parfois à la lecture (`NaN`) : ces mesures ne
sont jamais publiées.

## 5. Téléversement

```sh
pio run -t upload
pio device monitor -b 115200
```

Sortie attendue dans le moniteur série :

```
=== Capteur DHT22 -> MQTT (labo IoT) ===
[WiFi] Connexion a IoT-Lab....
[WiFi] Connecte, IP : 192.168.50.x
[WiFi] Veille WiFi : desactivee
[MQTT] Connexion a 192.168.50.182:1883 (client "capteur1")... OK
[MQTT] T=21.5 C (publie)  |  H=48.2 % (publie)
```

> Si `pio` n'est pas reconnu dans le terminal : voir la doc PlatformIO « Install Shell Commands »
> (liens symboliques dans `~/.local/bin`), ou utiliser le terminal intégré de PlatformIO dans VS Code.

## 6. Tests de validation

### Test 1 : réception des mesures

```sh
# Sur le Pi, ou sur un poste connecté à IoT-Lab
mosquitto_sub -h 192.168.50.182 -t 'labo/capteur1/#' -v
```

**Attendu**, toutes les 5 secondes :

```
labo/capteur1/temperature 21.5
labo/capteur1/humidite 48.2
```

### Test 2 : résilience à une coupure du broker

```sh
# Sur le Pi
sudo systemctl stop mosquitto    # le moniteur série affiche des échecs "state=..."
sudo systemctl start mosquitto   # le capteur doit se reconnecter seul en quelques secondes
```

**Attendu** : reprise automatique des publications, sans redémarrer l'ESP32.

### Résultats obtenus

| Test | Résultat |
|---|---|
| 1. Réception des mesures sur le broker | ✅ |
| 2. Reconnexion automatique après arrêt du broker | ✅ |

## 7. Ce que démontre cet objet

- Les mesures circulent **en clair** : elles sont lisibles par toute personne capable d'observer
  le trafic réseau.
- N'importe quel client anonyme peut **lire** les mesures (`mosquitto_sub -t '#'`).
- N'importe quel client anonyme peut **publier de fausses mesures** sur les mêmes topics : le
  broker ne vérifie pas l'identité de l'émetteur.

---

## Sources

- PubSubClient, documentation de l'API (v2.8) : https://pubsubclient.knolleary.net/api
- PubSubClient, en-tête source `PubSubClient.h` : https://github.com/knolleary/pubsubclient/blob/master/src/PubSubClient.h
- Registre PlatformIO, PubSubClient : https://registry.platformio.org/libraries/knolleary/PubSubClient
- Spécification MQTT 3.1.1 (OASIS), section 3.1.4 : http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html
- PlatformIO, *Install Shell Commands* : https://docs.platformio.org/en/latest/core/installation/shell-commands.html