#include <Arduino.h>
#include <DHT.h>

// --- Configuration du capteur ---
#define DHT_PIN  4        // la broche OUT du DHT22 est sur GPIO 4
#define DHT_TYPE DHT22    // on précise le modèle (DHT22, pas DHT11)

DHT dht(DHT_PIN, DHT_TYPE);   // on crée l'objet "dht" qui gère le capteur

void setup() {
  Serial.begin(115200);
  dht.begin();              // initialise le capteur
  Serial.println("Lecture du DHT22...");
}

void loop() {
  // Le DHT22 est lent : on attend 2 secondes entre deux lectures
  delay(2000);

  float humidite    = dht.readHumidity();      // lecture humidité (%)
  float temperature = dht.readTemperature();   // lecture température (°C)

  // Les premières lectures échouent parfois -> on vérifie avant d'utiliser
  if (isnan(humidite) || isnan(temperature)) {
    Serial.println("Echec de lecture du capteur !");
    return;   // on saute ce tour de boucle et on réessaie
  }

  // Affichage
  Serial.print("Temperature : ");
  Serial.print(temperature);
  Serial.print(" °C   |   Humidite : ");
  Serial.print(humidite);
  Serial.println(" %");
}