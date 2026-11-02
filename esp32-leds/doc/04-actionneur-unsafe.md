# Actionneur LED + bouton : commande à distance en MQTT

> ⚠️ **Objet volontairement vulnérable** : il obéit à n'importe quel message reçu, sans vérifier qui l'envoie.
> Il sert de cible pédagogique et ne doit être utilisé **que sur le réseau isolé `IoT-Lab`**.

## En une phrase

Un ESP32 avec deux LED (verte = allumé, rouge = éteint) et un bouton, que l'on peut commander
**de deux façons** : en appuyant sur le bouton, ou à distance en envoyant `ON` / `OFF` par MQTT.

## L'idée, simplement

Imagine une lampe connectée :

- l'**interrupteur** sur la lampe, c'est le **bouton** : il marche toujours, même sans réseau ;
- l'**application** sur ton téléphone, c'est la **commande MQTT** : elle envoie `ON` ou `OFF` à distance ;
- et la lampe **prévient** tout le monde de son état réel à chaque changement, pour que
  l'application affiche toujours la bonne information.

```
      Commande à distance                        Commande locale
   (mosquitto_pub, attaquant...)                    (bouton)
               │                                        │
               ▼                                        ▼
   labo/actionneur/commande ──► [ ESP32 ] ◄────────── appui
                                    │
                         LED verte (ON) / rouge (OFF)
                                    │
                                    ▼
                         labo/actionneur/etat ──► tous les abonnés
```

---

## Prérequis

| Élément | Valeur dans ce labo |
|---|---|
| Carte | ESP32 DevKit WROOM-32 |
| Environnement | VS Code + PlatformIO (framework Arduino) |
| Broker | Mosquitto sans authentification (voir `02-mosquitto-setup.md`), IP `192.168.50.182` |
| Réseau | WiFi `IoT-Lab` |

### Câblage

| Composant | Broche ESP32 | Remarque |
|---|---|---|
| LED verte | GPIO 26 | Avec une résistance en série |
| LED rouge | GPIO 25 | Avec une résistance en série |
| Bouton | GPIO 27 ↔ GND | Pas de résistance externe : le code active la résistance interne (*pull-up*) |

> **Pourquoi « appuyé = LOW » ?** Avec la résistance interne, la broche est maintenue à l'état
> haut (`HIGH`) au repos. Appuyer sur le bouton la relie à la masse : elle passe à l'état bas (`LOW`).

---

## 1. Le contrat MQTT de l'objet

C'est la « langue » que parle l'actionneur : ce qu'il écoute, et ce qu'il répond.

| Sens | Topic | Messages | Détail |
|---|---|---|---|
| **Il écoute** | `labo/actionneur/commande` | `ON` ou `OFF` | Exactement ces mots, en majuscules. Tout le reste est ignoré |
| **Il publie** | `labo/actionneur/etat` | `ON` ou `OFF` | À chaque changement, qu'il vienne du bouton ou de MQTT |

| Paramètre | Valeur | Pourquoi |
|---|---|---|
| Identifiant client | `actionneur1` | Doit être unique sur le broker, comme pour le capteur |
| État au démarrage | `OFF` (LED rouge) | Un objet qui redémarre revient à un état sûr |
| Message d'état retenu | Oui | Le broker garde le dernier état : un nouvel abonné le connaît immédiatement |
| Authentification | Aucune | C'est la vulnérabilité étudiée |

---

## 2. Mise en place

### Dépendance

Dans `platformio.ini`, ajouter PubSubClient :

```ini
lib_deps =
    knolleary/PubSubClient@^2.8
```

### Secrets

Comme pour le capteur, le mot de passe WiFi et l'adresse du broker sont hors du code source :

```sh
cp include/secrets.example.h include/secrets.h
# éditer include/secrets.h : WIFI_SSID, WIFI_PASSWORD, MQTT_HOST
git status   # secrets.h ne doit PAS apparaître
```

### Téléversement

```sh
pio run -t upload
pio device monitor -b 115200
```

Sortie attendue au démarrage :

```
=== Actionneur LED + bouton <-> MQTT (labo IoT) ===
[WiFi] Connexion a IoT-Lab....
[WiFi] Connecte, IP : 192.168.50.x
[WiFi] Veille WiFi : desactivee
[MQTT] Connexion a 192.168.50.182:1883 (client "actionneur1")... OK
[MQTT] Abonne a labo/actionneur/commande
[MQTT] Etat publie : labo/actionneur/etat OFF
```

---

## 3. Comment ça marche (sous le capot)

Le programme tourne en boucle, et à chaque tour il fait quatre choses, dans cet ordre :

1. **Regarder le bouton.** Toujours en premier : la commande locale fonctionne même sans réseau.
2. **Vérifier le WiFi**, et le relancer s'il est tombé.
3. **Vérifier la connexion au broker**, et la relancer si besoin.
4. **Traiter les messages reçus**, puis **publier l'état** s'il a changé.

Quelques points clés, expliqués simplement :

**Une seule porte d'entrée pour changer l'état.** Que l'ordre vienne du bouton ou de MQTT, il
passe par la même fonction (`setState`). Résultat : les LED et l'état publié sont toujours cohérents.

**Le bouton « rebondit ».** Un appui mécanique produit plusieurs contacts parasites en quelques
millisecondes. Le code n'accepte un changement que si le signal reste stable 50 ms (*anti-rebond*) :
un appui = une seule bascule.

**Rien n'est perdu pendant une coupure.** Si le bouton est pressé alors que le broker est
injoignable, la LED change tout de suite et l'état est publié **dès le retour** de la connexion.

**Il faut se réabonner après chaque reconnexion.** Le broker ne se souvient pas des abonnements
d'un client qui s'est déconnecté (session « propre »). L'actionneur se réabonne donc à chaque connexion.

**Le WiFi ne s'endort jamais** (`WiFi.setSleep(false)`). Un objet qui doit **recevoir** des
ordres doit rester à l'écoute. Sur le routeur du labo, la veille WiFi faisait perdre des messages
(problème mesuré sur le Raspberry Pi : 45 % de pertes avec la veille, 0 % sans).

**Le message reçu n'est pas une chaîne C classique.** Il n'est pas terminé par `\0` : le code
compare toujours en utilisant sa longueur. C'est un piège classique avec PubSubClient.

---

## 4. Tests de validation

Ouvrir un terminal d'observation, sur un poste connecté à `IoT-Lab` :

```sh
mosquitto_sub -h 192.168.50.182 -t 'labo/actionneur/#' -v
```

Puis envoyer des commandes depuis un second terminal :

```sh
mosquitto_pub -h 192.168.50.182 -t labo/actionneur/commande -m ON     # LED verte
mosquitto_pub -h 192.168.50.182 -t labo/actionneur/commande -m OFF    # LED rouge
mosquitto_pub -h 192.168.50.182 -t labo/actionneur/commande -m test   # ignorée
```

| # | Test | Attendu | Résultat |
|---|---|---|---|
| 1 | Envoyer `ON`, puis `OFF` | La LED change, et `labo/actionneur/etat` annonce le nouvel état | ✅ |
| 2 | Appuyer sur le bouton | La LED bascule, et l'état est publié | ✅ |
| 3 | Envoyer `test` | Rien ne change, le moniteur affiche « Commande inconnue » | ✅ |
| 4 | Nouvel abonné à `labo/actionneur/etat` | Reçoit immédiatement l'état actuel (message retenu) | ✅ |
| 5 | Arrêter le broker, appuyer, relancer le broker | La LED change tout de suite, l'état est publié à la reconnexion | ✅ |

Pour le test 5, sur le Pi :

```sh
sudo systemctl stop mosquitto
# appuyer sur le bouton : la LED change quand même
sudo systemctl start mosquitto
# l'état apparaît dans mosquitto_sub quelques secondes plus tard
```

---

## 5. Ce que démontre cet objet

Le test 1 est déjà une attaque. N'importe quel appareil connecté à `IoT-Lab` peut :

- **piloter l'objet** : une simple commande `mosquitto_pub ... -m OFF`, sans mot de passe, suffit
  à l'éteindre (attaque dite du **« faux OFF »**) ;
- **espionner son état** : `labo/actionneur/etat` est lisible par tous ;
- **mentir sur son état** : rien n'empêche un tiers de publier un faux `ON` sur le topic d'état.

L'actionneur ne peut pas distinguer son propriétaire d'un attaquant : il obéit au premier message
reçu. C'est exactement ce que les étapes suivantes du labo exploiteront, puis corrigeront
(authentification, chiffrement, identité machine).

> **Piste pour la suite (à vérifier lors de la phase d'attaque).** Une commande publiée en
> message retenu (`mosquitto_pub -r`) serait renvoyée par le broker à chaque reconnexion de
> l'actionneur : une commande « persistante ».

---

## Sources

- PubSubClient, documentation de l'API (v2.8) : https://pubsubclient.knolleary.net/api
- PubSubClient, en-tête source `PubSubClient.h` : https://github.com/knolleary/pubsubclient/blob/master/src/PubSubClient.h
- Registre PlatformIO, PubSubClient : https://registry.platformio.org/libraries/knolleary/PubSubClient
- Spécification MQTT 3.1.1 (OASIS) : sections 3.1.2.4 (*Clean Session*), 3.1.4 (identifiant client unique) et 3.3.1.3 (*RETAIN*) : http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/os/mqtt-v3.1.1-os.html