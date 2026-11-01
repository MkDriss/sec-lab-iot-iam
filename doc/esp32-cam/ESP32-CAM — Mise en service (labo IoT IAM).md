# ESP32-CAM — Mise en service (labo IoT/IAM)

Oct 28, 2026 · @Driss Meskini

## Objectif

La caméra fonctionne : elle a été flashée avec l’exemple officiel d’Espressif et son flux vidéo s’affiche dans un navigateur. C’était le seul objectif de cette première brique du labo.

À terme, cette ESP32-CAM jouera le rôle d’une fausse caméra IP volontairement vulnérable (flux accessible sans authentification), cible d’entraînement du labo IoT/IAM. Ce document couvre uniquement la mise en service : ni MQTT, ni sécurité, ni intégration réseau avancée.

## Matériel

- **ESP32-CAM AI-Thinker** : le module, qui réunit un microcontrôleur ESP32 et une caméra OV2640. Il n’a pas de port USB.
- **Base ESP32-CAM-MB** : elle alimente et flashe le module par micro-USB. Le module s’enfiche dessus, sans soudure.
- **Câble micro-USB « data »**.

Petite surprise : la puce USB de la base, sans marquage, se présente à Linux comme une puce FTDI FT232R. C’est très probablement un clone ([bug noyau Linux 220205](https://bugzilla.kernel.org/show_bug.cgi?id=220205)), mais il fonctionne normalement.

> Photo à insérer : le module et la base, puis le montage assemblé.

## Environnement

Fedora Linux, VS Code + PlatformIO, plateforme `espressif32` 8.0.0 avec le cœur Arduino 3.3.12 ([note de version](https://github.com/platformio/platform-espressif32/releases/tag/v8.0.0)).

## Étapes

1. **Créer le projet PlatformIO.** Dans VS Code, créer un nouveau projet avec la carte « AI Thinker ESP32-CAM » (identifiant `esp32cam`) et le framework Arduino ([documentation PlatformIO](https://github.com/platformio/platformio-docs/blob/develop/boards/espressif32/esp32cam.rst)). Compléter ensuite le fichier `platformio.ini` :

   ```ini
   [env:esp32cam]
   platform = espressif32 @ ~8.0.0
   board = esp32cam
   framework = arduino
   monitor_speed = 115200
   monitor_rts = 0
   monitor_dtr = 0
   upload_port  = /dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A5069RR4-if00-port0
   monitor_port = /dev/serial/by-id/usb-FTDI_FT232R_USB_UART_A5069RR4-if00-port0
   ```

   La version de la plateforme est figée pour que le projet reste reproductible. La PSRAM, indispensable à la caméra pour stocker ses images, est déjà activée par la définition de la carte : rien à ajouter. Les lignes `monitor_rts = 0` et `monitor_dtr = 0` sont nécessaires avec la base MB. Ces deux signaux y pilotent le reset de la puce : s’ils restent actifs, la puce est bloquée et le moniteur série reste muet ([forum PlatformIO](https://community.platformio.org/t/noob-stuck-on-esp32-cam-mb-with-pio-vscode/19117/8)). Enfin, les ports sont désignés par leur chemin `by-id`, qui ne change pas d’un branchement à l’autre.
2. **Récupérer le firmware officiel.** L’exemple CameraWebServer d’Espressif est déjà installé avec le cœur Arduino. Le trouver, puis copier tout son contenu dans `src/` (en supprimant le `main.cpp` vide) :

   ```bash
   find ~/.platformio/packages -type d -name CameraWebServer
   ```

   Dans `src/board_config.h`, commenter `CAMERA_MODEL_ESP_EYE`, actif par défaut, et décommenter `CAMERA_MODEL_AI_THINKER`. Un seul modèle doit être actif, sinon le firmware utiliserait les broches d’une autre carte.
3. **Renseigner le WiFi sans l’exposer.** Créer `include/wifi_secrets.h` et l’ajouter au `.gitignore` :

   ```cpp
   #pragma once
   #define WIFI_SSID     "IoT-Lab"
   #define WIFI_PASSWORD "********"
   ```

   Dans `CameraWebServer.ino`, ajouter `#include "wifi_secrets.h"` et remplacer les valeurs de `ssid` et `password` par `WIFI_SSID` et `WIFI_PASSWORD`. Une copie `wifi_secrets.example.h`, avec des valeurs factices, est versionnée pour montrer quoi remplir.
4. **Vérifier la connexion avant de flasher.** Enficher le module sur la base, USB débranché, puis brancher. Cette commande d’esptool interroge la puce sans rien écrire :

   ```bash
   ~/.platformio/packages/tool-esptool/esptool --port /dev/ttyUSB0 chip-id
   ```

   Résultat : la puce répond, une ESP32-D0WD-V3 (révision v3.1) avec un quartz de 40 MHz. La liaison USB → base → ESP32 fonctionne dans les deux sens.
5. **Compiler et flasher.**

   ```bash
   pio run
   pio run -t upload
   ```

   (Si `pio` n’est pas reconnu, utiliser `~/.platformio/penv/bin/pio`.) La base met la puce en mode flash toute seule, sans bouton. En cas d’échec : maintenir IO0, appuyer brièvement sur RST, relâcher IO0 et relancer l’upload ([Random Nerd Tutorials](https://randomnerdtutorials.com/upload-code-esp32-cam-mb-usb/)).
6. **Voir l’image.** Ouvrir le moniteur série avec `pio device monitor`, puis appuyer sur RST. La caméra redémarre, se connecte au WiFi « IoT-Lab » et affiche l’adresse de son serveur web. Ouvrir cette adresse dans un navigateur, puis lancer le flux vidéo depuis l’interface.

> Capture à insérer : l’interface web avec le flux vidéo.

## Résultats et preuves

Toutes les vérifications sont passées.

| Vérification | Méthode | Résultat |
| --- | --- | --- |
| Identification de la base | `dmesg` au branchement | Clone s’annonçant FTDI FT232R (0403:6001) |
| Communication série | `esptool chip-id` (lecture seule) | OK : ESP32-D0WD-V3 rev v3.1 détectée |
| Configuration PSRAM | Manifeste `esp32cam.json` | Flags PSRAM présents par défaut |
| Compilation et flash | `pio run`, puis `pio run -t upload` | OK, sans bouton |
| Flux vidéo | Navigateur sur l’adresse affichée | OK, image visible |

À archiver dans `docs/` du dépôt : la sortie `dmesg`, le manifeste `esp32cam.json` (extrait de la plateforme 8.0.0), la sortie `chip-id` et le log du moniteur série.

## Sources

Les sources primaires (documentation officielle, fichiers du projet) font foi. Les sources secondaires sont des retours d’expérience, utiles mais anecdotiques.

**Primaires**

- [PlatformIO — carte AI Thinker ESP32-CAM](https://github.com/platformio/platformio-docs/blob/develop/boards/espressif32/esp32cam.rst)
- [PlatformIO — note de version platform-espressif32 8.0.0](https://github.com/platformio/platform-espressif32/releases/tag/v8.0.0)
- Manifeste local `~/.platformio/platforms/espressif32/boards/esp32cam.json` et exemple CameraWebServer du cœur Arduino-ESP32 3.3.12
- [Linux kernel Bugzilla — bug 220205](https://bugzilla.kernel.org/show_bug.cgi?id=220205)

**Secondaires**

- [Random Nerd Tutorials — ESP32-CAM-MB](https://randomnerdtutorials.com/upload-code-esp32-cam-mb-usb/)
- [Forum PlatformIO — RTS/DTR avec la base MB](https://community.platformio.org/t/noob-stuck-on-esp32-cam-mb-with-pio-vscode/19117/8)
