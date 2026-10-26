#include <Arduino.h>
#include <WiFi.h>

const char* ssid     = "IoT-Lab";
const char* password = "LabIoT2026";

const int greenLedPin = 26;
const int redLedPin   = 25;

// Petite fonction pour refléter l'état sur les LED (évite de répéter le code)
void showConnected(bool connected) {
  digitalWrite(greenLedPin, connected ? HIGH : LOW);
  digitalWrite(redLedPin,   connected ? LOW  : HIGH);
}

void setup() {
  Serial.begin(115200);
  pinMode(greenLedPin, OUTPUT);
  pinMode(redLedPin, OUTPUT);

  // État de départ : pas connecté -> rouge
  showConnected(false);

  Serial.print("Connexion au WiFi ");
  Serial.print(ssid);
  WiFi.begin(ssid, password);

  while (WiFi.status() != WL_CONNECTED) {
    delay(500);
    Serial.print(".");
    // la LED reste rouge pendant toute l'attente
  }

  Serial.println();
  Serial.print("Connecté ! IP : ");
  Serial.println(WiFi.localIP());

  // Connecté -> vert
  showConnected(true);
}

void loop() {
  // On surveille en continu : si le WiFi tombe, on repasse au rouge
  bool connected = (WiFi.status() == WL_CONNECTED);
  showConnected(connected);
  delay(200);
}