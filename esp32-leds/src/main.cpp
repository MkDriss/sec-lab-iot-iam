/*
 * ============================================================================
 *  Actionneur LED + bouton <-> MQTT  |  Labo IoT (cible VOLONTAIREMENT vulnerable)
 * ============================================================================
 *  Role :
 *   - S'abonne a labo/actionneur/commande : "ON" -> LED verte, "OFF" -> LED rouge.
 *   - Le bouton physique bascule l'etat localement (commande locale, sans reseau).
 *   - Publie l'etat reel sur labo/actionneur/etat ("ON" / "OFF", message retenu)
 *     a chaque changement, qu'il vienne du bouton ou de MQTT.
 *
 *  Aucune authentification : n'importe quel client du reseau IoT-Lab peut
 *  commander l'objet. C'est la vulnerabilite etudiee (attaque "faux OFF").
 *
 *  Materiel : ESP32 DevKit WROOM-32
 *    - LED verte sur GPIO 26, LED rouge sur GPIO 25 (avec resistances)
 *    - Bouton entre GPIO 27 et GND (pull-up interne : appuye = LOW)
 *  Dependances : PubSubClient (voir platformio.ini).
 * ============================================================================
 */

#include <Arduino.h>
#include <WiFi.h>
#include <PubSubClient.h>

#include "secrets.h" // WIFI_SSID, WIFI_PASSWORD, MQTT_HOST -- NON versionne (.gitignore)

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------
const int GREEN_LED_PIN = 26;
const int RED_LED_PIN = 25;
const int BUTTON_PIN = 27;

const uint16_t MQTT_PORT = 1883;                         // MQTT non chiffre
const char *MQTT_CLIENT_ID = "actionneur1";              // DOIT etre unique par objet
const char *TOPIC_COMMANDE = "labo/actionneur/commande"; // ce qu'on ecoute
const char *TOPIC_ETAT = "labo/actionneur/etat";         // ce qu'on publie

const unsigned long RECONNECT_INTERVAL_MS = 5000; // pause entre 2 tentatives de reconnexion
const unsigned long WIFI_BOOT_TIMEOUT_MS = 15000; // attente max du WiFi au demarrage
const unsigned long DEBOUNCE_MS = 50;             // duree de stabilite exigee pour le bouton

// ---------------------------------------------------------------------------
// Objets et etat global
// ---------------------------------------------------------------------------
WiFiClient wifiClient;         // couche TCP
PubSubClient mqtt(wifiClient); // couche MQTT, au-dessus du TCP

bool isOn = false;               // etat au demarrage : OFF (etat sur par defaut)
bool statePublishPending = true; // l'etat doit etre publie des que MQTT est disponible

unsigned long lastWifiAttemptMs = 0;
unsigned long lastMqttAttemptMs = 0;

// Anti-rebond du bouton
int lastButtonReading = HIGH;         // derniere lecture brute
int stableButtonState = HIGH;         // dernier etat considere comme stable
unsigned long lastButtonChangeMs = 0; // instant du dernier changement brut

// ---------------------------------------------------------------------------
// Etat de l'objet
// ---------------------------------------------------------------------------

// Reflete l'etat sur les LED : verte = ON, rouge = OFF.
void showState()
{
  digitalWrite(GREEN_LED_PIN, isOn ? HIGH : LOW);
  digitalWrite(RED_LED_PIN, isOn ? LOW : HIGH);
}

// Point d'entree UNIQUE pour changer l'etat, quelle que soit la source.
void setState(bool newState, const char *source)
{
  if (newState == isOn)
  {
    Serial.printf("[ETAT] Deja %s (demande : %s), rien a faire\n", isOn ? "ON" : "OFF", source);
    return;
  }
  isOn = newState;
  showState();
  statePublishPending = true; // la publication se fera dans loop(), pas ici
  Serial.printf("[ETAT] -> %s (source : %s)\n", isOn ? "ON" : "OFF", source);
}

// Publie l'etat s'il a change et que le broker est joignable.
// Si le broker etait injoignable au moment du changement, l'etat sera publie
// automatiquement des la reconnexion.
void publishStateIfPending()
{
  if (!statePublishPending || !mqtt.connected())
    return;

  const char *payload = isOn ? "ON" : "OFF";

  // retained = true : le broker conserve ce dernier etat et l'envoie
  // immediatement a tout nouveau client qui s'abonne a TOPIC_ETAT.
  if (mqtt.publish(TOPIC_ETAT, payload, true))
  {
    statePublishPending = false;
    Serial.printf("[MQTT] Etat publie : %s %s\n", TOPIC_ETAT, payload);
  }
  else
  {
    Serial.println("[MQTT] Echec de publication de l'etat, nouvel essai au prochain tour");
  }
}

// ---------------------------------------------------------------------------
// Bouton (commande locale)
// ---------------------------------------------------------------------------

// Anti-rebond non bloquant : un changement n'est pris en compte que si la lecture
// reste stable pendant DEBOUNCE_MS. On bascule l'etat sur le front d'appui.
void handleButton()
{
  int reading = digitalRead(BUTTON_PIN);

  if (reading != lastButtonReading)
  {                                // le signal bouge (appui ou rebond)
    lastButtonChangeMs = millis(); // on relance le chronometre
    lastButtonReading = reading;
  }

  if (millis() - lastButtonChangeMs >= DEBOUNCE_MS && reading != stableButtonState)
  {
    stableButtonState = reading;
    if (stableButtonState == LOW)
    { // appui confirme (pull-up : appuye = LOW)
      setState(!isOn, "bouton");
    }
  }
}

// ---------------------------------------------------------------------------
// MQTT : reception des commandes
// ---------------------------------------------------------------------------

// Appelee par PubSubClient (depuis mqtt.loop()) a chaque message recu.
// ATTENTION : payload n'est PAS termine par '\0' : on utilise toujours length.
void onMqttMessage(char *topic, byte *payload, unsigned int length)
{
  Serial.printf("[MQTT] Recu sur %s : \"%.*s\"\n", topic, (int)length, (const char *)payload);

  if (strcmp(topic, TOPIC_COMMANDE) != 0)
    return; // on verifie que le topic est bien le topic de commande

  // Comparaison exacte et sensible a la casse : seuls "ON" et "OFF" sont acceptes.
  if (length == 2 && memcmp(payload, "ON", 2) == 0)
  {
    setState(true, "MQTT");
  }
  else if (length == 3 && memcmp(payload, "OFF", 3) == 0)
  {
    setState(false, "MQTT");
  }
  else
  {
    Serial.println("[MQTT] Commande inconnue, ignoree (attendu : ON ou OFF)");
  }
}

// ---------------------------------------------------------------------------
// WiFi
// ---------------------------------------------------------------------------

// Connexion initiale, appelee une seule fois dans setup().
void startWifi()
{
  WiFi.mode(WIFI_STA);
  // Desactive la veille WiFi : un objet qui RECOIT des commandes doit rester
  // a l'ecoute. Sur le routeur du labo, la veille fait perdre des trames entrantes.
  WiFi.setSleep(false); // desactive le sleep automatique pour ameliorer la stabilite de la connection
  WiFi.begin(WIFI_SSID, WIFI_PASSWORD);

  Serial.printf("[WiFi] Connexion a %s", WIFI_SSID);
  unsigned long start = millis();
  while (WiFi.status() != WL_CONNECTED && millis() - start < WIFI_BOOT_TIMEOUT_MS)
  {
    delay(500); // acceptable ici : on est dans setup()
    Serial.print(".");
  }
  Serial.println();

  if (WiFi.status() == WL_CONNECTED)
  {
    Serial.printf("[WiFi] Connecte, IP : %s\n", WiFi.localIP().toString().c_str());
  }
  else
  {
    Serial.println("[WiFi] Echec au demarrage, nouvelles tentatives dans loop()");
  }
}

// Relance le WiFi s'il est tombe, SANS bloquer. Retourne true si le WiFi est disponible.
bool ensureWifi()
{
  if (WiFi.status() == WL_CONNECTED)
    return true;

  unsigned long now = millis();
  if (now - lastWifiAttemptMs >= RECONNECT_INTERVAL_MS)
  {
    lastWifiAttemptMs = now;
    Serial.println("[WiFi] Deconnecte, tentative de reconnexion...");
    WiFi.reconnect();
  }
  return false;
}

// ---------------------------------------------------------------------------
// MQTT : connexion
// ---------------------------------------------------------------------------

// (Re)connecte au broker si besoin. Retourne true si la session MQTT est active.
bool ensureMqtt()
{
  if (mqtt.connected())
    return true;

  unsigned long now = millis();
  if (now - lastMqttAttemptMs < RECONNECT_INTERVAL_MS)
    return false;
  lastMqttAttemptMs = now;

  Serial.printf("[MQTT] Connexion a %s:%u (client \"%s\")... ", MQTT_HOST, MQTT_PORT, MQTT_CLIENT_ID);

  // Connexion ANONYME : ni identifiant ni mot de passe (vulnerabilite voulue).
  if (!mqtt.connect(MQTT_CLIENT_ID))
  {
    Serial.printf("echec, state=%d\n", mqtt.state());
    return false;
  }
  Serial.println("OK");

  // La session est "propre" (clean session) : le broker ne garde pas nos
  // abonnements entre deux connexions. Il faut donc se reabonner a CHAQUE connexion.
  if (mqtt.subscribe(TOPIC_COMMANDE))
  {
    Serial.printf("[MQTT] Abonne a %s\n", TOPIC_COMMANDE);
  }
  else
  {
    Serial.println("[MQTT] Echec de l'abonnement");
  }

  // Republie l'etat a chaque (re)connexion : le broker reflete toujours l'etat reel,
  // y compris s'il a change via le bouton pendant une coupure.
  statePublishPending = true;
  return true;
}

// ---------------------------------------------------------------------------
// Programme principal
// ---------------------------------------------------------------------------
void setup()
{
  Serial.begin(115200);
  delay(200);
  Serial.println("\n=== Actionneur LED + bouton <-> MQTT (labo IoT) ===");

  pinMode(GREEN_LED_PIN, OUTPUT);
  pinMode(RED_LED_PIN, OUTPUT);
  pinMode(BUTTON_PIN, INPUT_PULLUP);
  showState(); // etat initial OFF -> LED rouge

  startWifi();
  Serial.printf("[WiFi] Veille WiFi : %s\n", WiFi.getSleep() ? "ACTIVE (anormal)" : "desactivee");

  // Adresse du broker : une IP (reservee en DHCP sur le routeur), pas de DNS necessaire.
  IPAddress brokerIp;
  if (!brokerIp.fromString(MQTT_HOST))
  {
    Serial.printf("[MQTT] MQTT_HOST invalide : \"%s\" (une adresse IP est attendue)\n", MQTT_HOST);
    Serial.println("Impossible de poursuivre...");
    while (true)
    {
      Serial.println(".");
      delay(10000); // erreur de configuration : on s'arrete ici
    }
  }
  mqtt.setServer(brokerIp, MQTT_PORT);
  mqtt.setCallback(onMqttMessage);
}

void loop()
{
  // Le bouton est traite en premier et a chaque tour : la commande locale
  // fonctionne meme sans WiFi ni broker.
  handleButton();

  if (!ensureWifi())
    return;
  if (!ensureMqtt())
    return;

  mqtt.loop();             // recoit les commandes (appelle onMqttMessage) + keepalive
  publishStateIfPending(); // publie l'etat s'il a change
}