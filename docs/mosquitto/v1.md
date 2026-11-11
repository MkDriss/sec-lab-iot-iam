# Mise en place du broker MQTT (Mosquitto)

> ⚠️ **Configuration volontairement vulnérable** : MQTT en clair, sans authentification.
> Elle sert de cible pédagogique au labo et ne doit être utilisée **que sur le réseau isolé `IoT-Lab`**.

## Objectif

Installer Mosquitto sur le Raspberry Pi et le rendre joignable depuis le réseau `IoT-Lab`
(192.168.50.0/24), **en clair et sans authentification**. Ce broker sera la cible des étapes
suivantes du labo (sniffing, interception, publication de commandes non autorisées).

## Prérequis

| Élément | Valeur dans ce labo |
|---|---|
| Broker | Raspberry Pi 4 sous Raspberry Pi OS (Debian), nom d'hôte `pipy` |
| Réseau du Pi | Connecté au WiFi `IoT-Lab`, IP réservée en DHCP : `192.168.50.182` (voir la doc du routeur) |
| Accès | SSH vers le Pi |
| Poste de test | Un client connecté au WiFi `IoT-Lab` (ici un ThinkPad sous Fedora) |
| Accès Internet | Nécessaire pour `apt` et `dnf`. **La zone `iot` du labo n'a pas d'accès Internet** (le routeur n'a pas de lien WAN) : installer les paquets **avant** de connecter les machines à `IoT-Lab`, ou depuis un réseau qui a Internet |

---

## 1. Installer Mosquitto sur le Pi

```sh
sudo apt update
sudo apt install mosquitto mosquitto-clients
```

- `mosquitto` : le broker.
- `mosquitto-clients` : les outils en ligne de commande `mosquitto_pub` et `mosquitto_sub`.

**Vérification :**

```sh
mosquitto -h | head -n 1              # attendu : mosquitto version 2.x (ici 2.0.21)
systemctl is-enabled mosquitto        # attendu : enabled (démarre au boot)
sudo ss -tlnp | grep 1883             # attendu : 127.0.0.1:1883 et [::1]:1883
```

> **Pourquoi seulement `127.0.0.1` ?** Depuis la version 2.0, un broker lancé sans listener
> configuré se lie uniquement aux interfaces de boucle locale : seules les connexions depuis
> la machine elle-même sont possibles. C'est une sécurité par défaut qu'on va désactiver
> volontairement. *(Source : annonce de Mosquitto 2.0.0)*

## 2. Vérifier que le dossier `conf.d` est chargé

```sh
grep -n "include_dir" /etc/mosquitto/mosquitto.conf
# attendu : include_dir /etc/mosquitto/conf.d
```

La directive `include_dir` charge tous les fichiers `.conf` du dossier indiqué. On place donc
la configuration du labo dans un fichier séparé, sans modifier `mosquitto.conf` livré par le
paquet. Elle reste ainsi isolée, lisible et facile à retirer. *(Source : `man mosquitto.conf`)*

## 3. Créer la configuration du labo

```sh
sudo tee /etc/mosquitto/conf.d/labo.conf > /dev/null << 'EOF'
# =====================================================================
# Labo IoT : configuration VOLONTAIREMENT vulnérable
# MQTT en clair, sans authentification (cible pédagogique)
# À n'utiliser que sur le réseau isolé IoT-Lab
# =====================================================================

# Écoute sur toutes les interfaces, port MQTT standard (non chiffré)
listener 1883

# Accepte les clients sans identifiant : AUCUNE authentification
allow_anonymous true
EOF
```

| Directive | Effet | Comportement par défaut (Mosquitto 2.x) |
|---|---|---|
| `listener 1883` | Ouvre le port 1883 sur toutes les interfaces (`0.0.0.0` et `::`) | Sans listener : boucle locale uniquement |
| `allow_anonymous true` | Accepte les clients sans nom d'utilisateur | `false` sur tout listener configuré |

> La documentation officielle précise que `allow_anonymous true` ne devrait être utilisé
> qu'en cas de besoin spécifique de clients non authentifiés. C'est précisément le cas ici :
> l'absence d'authentification **est** la vulnérabilité étudiée.
> *(Source : « Migrating from 1.x to 2.0 »)*

## 4. Redémarrer et vérifier

```sh
sudo systemctl restart mosquitto
systemctl status mosquitto --no-pager             # attendu : active (running)
sudo ss -tlnp | grep 1883                         # attendu : 0.0.0.0:1883 et [::]:1883
sudo tail -n 20 /var/log/mosquitto/mosquitto.log  # messages de démarrage
```

Si le fichier de log n'existe pas : `sudo journalctl -u mosquitto -n 20`.

---

## 5. Tests de validation

### Préparation du poste de test

À faire **pendant que le poste a encore accès à Internet** :

```sh
# Fedora : trouver le paquet qui fournit les clients MQTT, puis l'installer
dnf provides '*/mosquitto_pub'
sudo dnf install <paquet_trouvé>
```

Connecter ensuite le poste au WiFi `IoT-Lab`. Les tests ci-dessous utilisent deux terminaux
(un onglet SSH vers le Pi, un onglet local sur le poste de test).

### Test 1 : en local sur le Pi

Vérifie que le broker fonctionne, indépendamment du réseau.

```sh
# Terminal 1 (Pi) : écoute tous les topics du labo
mosquitto_sub -h localhost -t 'labo/#' -v

# Terminal 2 (Pi) : publie un message
mosquitto_pub -h localhost -t labo/test -m "bonjour depuis le Pi"
```

**Attendu** dans le terminal 1 : `labo/test bonjour depuis le Pi`

### Test 2 : publication depuis le réseau

Vérifie qu'un appareil de `IoT-Lab` peut **publier** vers le broker (chemin qu'emprunteront les ESP32).

```sh
# Pi : on écoute
mosquitto_sub -h localhost -t 'labo/#' -v

# Poste de test : on publie à travers le réseau
mosquitto_pub -h 192.168.50.182 -t labo/test -m "bonjour depuis le ThinkPad"
```

**Attendu** sur le Pi : `labo/test bonjour depuis le ThinkPad`

### Test 3 : lecture anonyme de tout le trafic (démonstration de la vulnérabilité)

Vérifie qu'un client **anonyme** peut s'abonner à **tous** les topics (`#`) et lire ce qui transite.

```sh
# Poste de test : un client anonyme écoute TOUT
mosquitto_sub -h 192.168.50.182 -t '#' -v

# Pi : on simule une mesure de capteur
mosquitto_pub -h localhost -t labo/capteur1/temperature -m "21.5"
```

**Attendu** sur le poste de test : `labo/capteur1/temperature 21.5`

### Résultats obtenus

| Test | Résultat |
|---|---|
| 1. Local sur le Pi | ✅ |
| 2. Publication depuis `IoT-Lab` | ✅ |
| 3. Lecture anonyme de tous les topics | ✅ |

---

## 6. Ce que démontre cette configuration

Avec cette configuration, **n'importe quel appareil connecté à `IoT-Lab`** peut :

- se connecter au broker **sans aucune identité** ;
- **lire** tous les messages (`#`), y compris les mesures des capteurs ;
- **publier** sur n'importe quel topic, par exemple envoyer un faux `OFF` à l'actionneur ;
- **observer le trafic sur le réseau**, puisque MQTT circule en clair sur TCP 1883.

Ces faiblesses sont la base des étapes suivantes du labo, puis de leur correction
(authentification, chiffrement TLS, identité machine).

## 7. Revenir à la configuration sécurisée par défaut

```sh
sudo rm /etc/mosquitto/conf.d/labo.conf
sudo systemctl restart mosquitto
sudo ss -tlnp | grep 1883     # attendu : de nouveau 127.0.0.1:1883 uniquement
```

## Dépannage

| Symptôme côté client | Cause probable | Vérification |
|---|---|---|
| `Connection refused` | Broker arrêté, ou toujours en mode local uniquement | `systemctl status mosquitto` et `sudo ss -tlnp \| grep 1883` |
| `No route to host` | Problème réseau, pas Mosquitto (le Pi ne répond pas sur `IoT-Lab`) | `ping 192.168.50.182`, voir la doc de diagnostic WiFi du Pi |
| Erreur « not authorized » | `allow_anonymous` absent ou à `false` | Contenu de `/etc/mosquitto/conf.d/labo.conf`, puis redémarrage |

---

## Sources

- Mosquitto, *Migrating from 1.x to 2.0* : https://mosquitto.org/documentation/migrating-to-2-0/
- Mosquitto, *Version 2.0.0 released* : https://mosquitto.org/blog/2020/12/version-2-0-0-released/
- Page de manuel `mosquitto.conf(5)` (`listener`, `allow_anonymous`, `include_dir`) : https://mosquitto.org/man/mosquitto-conf-5.html, ou `man mosquitto.conf` sur le Pi
- Dépôt Debian de Mosquitto, paquets `mosquitto` et `mosquitto-clients` : https://repo.mosquitto.org/debian/README.txt