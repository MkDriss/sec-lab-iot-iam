#pragma once
// ============================================================================
//  MODELE de fichier de secrets.
//  1. Copier ce fichier en "include/secrets.h"
//  2. Renseigner les vraies valeurs dans secrets.h
//  secrets.h est liste dans .gitignore : il ne doit JAMAIS etre commite.
//  Ce fichier-ci (secrets.example.h) peut etre commite : il ne contient rien de sensible.
// ============================================================================

#define WIFI_SSID     "IoT-Lab"
#define WIFI_PASSWORD "remplacer-par-le-mot-de-passe"

// IP du broker Mosquitto (Raspberry Pi), reservee en DHCP sur le routeur
#define MQTT_HOST     "192.168.50.182"