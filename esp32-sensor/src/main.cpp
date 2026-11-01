/*
 * ============================================================================
 *  Capteur DHT22 -> MQTT  |  Labo IoT (cible VOLONTAIREMENT vulnerable)
 * ============================================================================
 *  Role : lire temperature/humidite sur un DHT22 et les publier en MQTT,
 *         EN CLAIR et SANS AUTHENTIFICATION, vers le broker Mosquitto du Pi.
 *
 *  Topics publies (payload = nombre en texte, 1 decimale) :
 *    labo/capteur1/temperature   -> degres Celsius,           ex. "21.5"
 *    labo/capteur1/humidite      -> % d'humidite relative,    ex. "48.2"
 *
 *  Materiel     : ESP32 DevKit WROOM-32, broche OUT du DHT22 sur GPIO 4.
 *  Dependances  : PubSubClient + DHT sensor library (voir platformio.ini).
 * ============================================================================
 */

#include <Arduino.h>
#include <WiFi.h>
#include <PubSubClient.h>
#include <DHT.h>
#include "secrets.h" 

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------
#define DHT_PIN  4
#define DHT_TYPE DHT22

const uint16_t MQTT_PORT      = 1883;                          // MQTT non chiffre
const char*    MQTT_CLIENT_ID = "capteur1";                    // DOIT etre unique par objet
const char*    TOPIC_TEMP     = "labo/capteur1/temperature";
const char*    TOPIC_HUM      = "labo/capteur1/humidite";

const unsigned long PUBLISH_INTERVAL_MS   = 5000;   // 1 mesure / 5 s (le DHT22 exige >= 2 s)
const unsigned long RECONNECT_INTERVAL_MS = 5000;   // pause entre 2 tentatives de reconnexion
const unsigned long WIFI_BOOT_TIMEOUT_MS  = 15000;  // attente max du WiFi au demarrage

// ---------------------------------------------------------------------------
// Objets globaux
// ---------------------------------------------------------------------------
DHT          dht(DHT_PIN, DHT_TYPE);
WiFiClient   wifiClient;          // couche TCP (fournie par le WiFi de l'ESP32)
PubSubClient mqtt(wifiClient);    // couche MQTT, qui parle a travers ce TCP

unsigned long lastPublishMs     = 0;
unsigned long lastWifiAttemptMs = 0;
unsigned long lastMqttAttemptMs = 0;

// ---------------------------------------------------------------------------
// WiFi
// ---------------------------------------------------------------------------

// Connexion initiale, appelee une seule fois dans setup().
void startWifi() {
  WiFi.mode(WIFI_STA);
  // Desactive la veille WiFi : sur le routeur du labo, la veille fait perdre
  // des trames entrantes (meme probleme que celui diagnostique sur le Pi).
  WiFi.setSleep(false);
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);

  Serial.printf("[WiFi] Connexion a %s", WIFI_SSID);
  unsigned long start = millis();
  while (WiFi.status() != WL_CONNECTED && millis() - start < WIFI_BOOT_TIMEOUT_MS) {
    delay(500);   // acceptable ici : on est dans setup(), la session MQTT n'existe pas encore
    Serial.print(".");
  }
  Serial.println();

  if (WiFi.status() == WL_CONNECTED) {
    Serial.printf("[WiFi] Connecte, IP : %s\n", WiFi.localIP().toString().c_str());
  } else {
    Serial.println("[WiFi] Echec au demarrage, nouvelles tentatives dans loop()");
  }
}

// Appelee a chaque tour de loop() : relance le WiFi s'il est tombe, SANS bloquer.
// Retourne true si le WiFi est disponible.
bool ensureWifi() {
  if (WiFi.status() == WL_CONNECTED) return true;

  unsigned long now = millis();
  if (now - lastWifiAttemptMs >= RECONNECT_INTERVAL_MS) {
    lastWifiAttemptMs = now;
    Serial.println("[WiFi] Deconnecte, tentative de reconnexion...");
    WiFi.reconnect();
  }
  return false;
}

// ---------------------------------------------------------------------------
// MQTT
// ---------------------------------------------------------------------------

// Appelee a chaque tour de loop() : (re)connecte au broker si besoin, SANS bloquer
// entre deux tentatives. Retourne true si la session MQTT est active.
bool ensureMqtt() {
  if (mqtt.connected()) return true;

  unsigned long now = millis();
  if (now - lastMqttAttemptMs < RECONNECT_INTERVAL_MS) return false;
  lastMqttAttemptMs = now;

  Serial.printf("[MQTT] Connexion a %s:%u (client \"%s\")... ", MQTT_HOST, MQTT_PORT, MQTT_CLIENT_ID);

  // Connexion ANONYME : ni identifiant ni mot de passe (vulnerabilite voulue).
  if (mqtt.connect(MQTT_CLIENT_ID)) {
    Serial.println("OK");
    return true;
  }

  // state() renvoie un code d'erreur (voir la doc API de PubSubClient).
  Serial.printf("echec, state=%d\n", mqtt.state());
  return false;
}

// Lit le DHT22 et publie les deux mesures.
void publishMeasures() {
  float humidite    = dht.readHumidity();
  float temperature = dht.readTemperature();   // en degres Celsius

  // Les lectures du DHT22 echouent parfois : on ne publie jamais une valeur invalide.
  if (isnan(humidite) || isnan(temperature)) {
    Serial.println("[DHT22] Echec de lecture, mesure ignoree");
    return;
  }

  char payload[16];   // largement suffisant pour "-40.0" ou "100.0"

  snprintf(payload, sizeof(payload), "%.1f", temperature);
  bool okTemp = mqtt.publish(TOPIC_TEMP, payload);   // QoS 0, message non retenu

  snprintf(payload, sizeof(payload), "%.1f", humidite);
  bool okHum = mqtt.publish(TOPIC_HUM, payload);

  Serial.printf("[MQTT] T=%.1f C (%s)  |  H=%.1f %% (%s)\n",
                temperature, okTemp ? "publie" : "ECHEC",
                humidite,    okHum  ? "publie" : "ECHEC");
}

// ---------------------------------------------------------------------------
// Programme principal
// ---------------------------------------------------------------------------
void setup() {
  Serial.begin(115200);
  delay(200);
  Serial.println("\n=== Capteur DHT22 -> MQTT (labo IoT) ===");

  dht.begin();
  startWifi();

  // Verification : la veille WiFi doit etre desactivee.
  Serial.printf("[WiFi] Veille WiFi : %s\n", WiFi.getSleep() ? "ACTIVE (anormal)" : "desactivee");

  // Adresse du broker : une IP (reservee en DHCP sur le routeur), pas de DNS necessaire.
  IPAddress brokerIp;
  if (!brokerIp.fromString(MQTT_HOST)) {
    Serial.printf("[MQTT] MQTT_HOST invalide : \"%s\" (une adresse IP est attendue)\n", MQTT_HOST);
    while (true) delay(1000);   // erreur de configuration : on s'arrete ici
  }
  mqtt.setServer(brokerIp, MQTT_PORT);
}

void loop() {
  if (!ensureWifi()) return;   // pas de WiFi : rien d'autre a faire pour ce tour
  if (!ensureMqtt()) return;   // pas de broker : idem

  // Entretient la session (keepalive, paquets entrants). Doit etre appele souvent :
  // c'est pour cela qu'on n'utilise jamais de long delay() dans loop().
  mqtt.loop();

  unsigned long now = millis();
  if (now - lastPublishMs >= PUBLISH_INTERVAL_MS) {
    lastPublishMs = now;
    publishMeasures();
  }
}