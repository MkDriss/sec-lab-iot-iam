# DOC Config Router

## 1 Obtenir un acces ssh

1 - aller sur la page 192.168.8.1 pour configurer le router
2 - changer le mdp par defaut 
3 - se connecter en ssh pour config le sous reseau wifi (permet d'isoler le Pi et les esp32 sur un reseau dedie)

## 2 Configurer un point d'acces dedie au lab

### 2a — Créer l’interface réseau iot

```sh
uci set network.iot=interface # configurer une interface
uci set network.iot.proto='static' # utilise une ip statique
uci set network.iot.ipaddr='192.168.50.1' # addresse ip de l'interface
uci set network.iot.netmask='255.255.255.0' # subnet mask
uci set network.iot.type='bridge' # defininir l'interface comme un pont
uci set network.iot.ifname='ra1' # definir le npm de l'interface
uci commit network # valider
/etc/init.d/network reload
```

On vient créer une nouvelle « pièce » réseau nommée iot, dont l’adresse de passerelle est 192.168.50.1.s

**Source(s):**
- *https://openwrt.org/docs/guide-user/network/wifi/basic*
- *openwrt.org/docs/guide-user/base-system/uci*


### 2b — Donner un DHCP à cette interface

Sans ça, les objets qui rejoindront le réseau iot n’obtiendront pas d’adresse IP automatiquement.

```sh
uci set dhcp.iot=dhcp # definir une config dhcp
uci set dhcp.iot.interface='iot' # rattacher la config a l'objet iot
uci set dhcp.iot.start='100' # port de depart
uci set dhcp.iot.limit='150'  # port de fin
uci set dhcp.iot.leasetime='12h' # temps devalidite d'une ip
uci commit dhcp # valider
/etc/init.d/dnsmasq restart
```

**Source(s):** 
- *openwrt.org/docs/guide-user/base-system/dhcp*

les objets sur iot recevront une IP entre 192.168.50.100 et 192.168.50.249.

### 2c — Créer le SSID WiFi et le rattacher à iot

D’abord, il faut reperer le nom de la radio 2,4 GHz :

```sh
uci show wireless | grep -E "wifi-device|\.band"
```

```sh
# output
wireless.mt7628=wifi-device # -> RADIO_NAME = mt7628
wireless.mt7628.band='2g'
```

```sh
uci set wireless.iotwifi=wifi-iface # definir une interface wifi
uci set wireless.iotwifi.device='RADIO_NAME' #nom du dispositif radio pour le wifi
uci set wireless.iotwifi.mode='ap' # setup en mode access point
uci set wireless.iotwifi.network='iot' # on le rattache au reseau qu'on a cree juste avant
uci set wireless.iotwifi.ifname='ra1' # brancher l'interface wifi sur l'interface réseau virtuelle
uci set wireless.iotwifi.encryption='psk2' # niveau de securite du wifi -> WPA2 pour particulier, standard depuis 2004 
                                           # meme si en 2018 on a eu du WPA3 (sae) qui est devenu le nouveau standard 
uci set wireless.iotwifi.ssid='IoT-Lab' # nom du reseau
uci set wireless.iotwifi.key='choisis-un-mot-de-passe-solide' # doit faire plus de 8 char sinon il ne sera pas possible de se connecter (a cause du psk2)
uci commit wireless # valider la config
wifi reload 
```
Penser a bien tout relancer 

```sh
/etc/init.d/network restart
/etc/init.d/dnsmasq restart
wifi reload
```

**Source(s):**
- *https://openwrt.org/docs/guide-user/network/network_configuration*

## 3 Configurer le pare feu

le pare feu d'OpenWrt utilise des "zone" dans lequel on peut ranger des reseaux puis decider du flux dans chacune des zones

le LAN par defaut a deja sa propre zone, il faut donc en creer une 2eme le Lab Iot

Politique cible

iot -> lan : interdit | le mur. Un objet piraté ne doit pas atteindre ta maison/ton PC.
lan -> iot : autorisé | toi, depuis le réseau de confiance, tu peux administrer et joindre les objets.
iot -> wan : autorisé | si un jour tu branches Internet, les objets pourront sortir (optionnel, mais sans effet tant que tu n’as pas de WAN).
iot -> routeur lui-même : juste DHCP + DNS | les objets doivent pouvoir obtenir une IP et résoudre des noms, mais rien de plus (pas d’accès à l’admin du routeur)


### 3.a Creer une zone et une config globale

```sh
uci add firewall zone # create a new firewall zone
uci set firewall.@zone[-1].name='iot' # name the last zone (@zone[-1] finds the last created zone) and give it the name iot
uci set firewall.@zone[-1].network='iot' # apply the zone to the network configured before: 'iot'
uci set firewall.@zone[-1].input='REJECT' # bloque tout le contenu direct vers le routeur
uci set firewall.@zone[-1].output='ACCEPT' # accepte que des informations du routeur puissent lui parvenir
uci set firewall.@zone[-1].forward='REJECT' # bloque les messages qui sont forwarde par le routeur
```

### 3.b Autoriser Iot vers WAN (internet)

```sh
uci add firewall forwarding # ajout d'une regle de forwarding
uci set firewall.@forwarding[-1].src='iot' # configure le source du forward
uci set firewall.@forwarding[-1].dest='wan' # configure la dest du forward

```
On a donc Iot -> WAN

### 3.c Autoriser Lan vers IoT 

```sh
uci add firewall forwarding
uci set firewall.@forwarding[-1].src='lan'
uci set firewall.@forwarding[-1].dest='iot'
```

### Fine tune la config pour autoriser Iot -> DHCP & Iot -> DNS

Cette etape est necessaire car elle va permettre aux objets du reseau de venir recuperer un adresse ip aupres du service DHCP du routeur mais egalemet de profiter du DNS de ce dernier.

```sh
# DHCP (port 67/udp)
uci add firewall rule # creer une nouvelle rule
uci set firewall.@rule[-1].name='Allow-DHCP-iot' # on la nome -> atteindre la derniere rule creee: firewall.@rule[-1]
uci set firewall.@rule[-1].src='iot' # definit la source venant de la zone 'iot'
uci set firewall.@rule[-1].proto='udp' # protocole udp
uci set firewall.@rule[-1].dest_port='67' # sur le port 67 (port par defaut DHCP)
uci set firewall.@rule[-1].target='ACCEPT' # on accepte sur la target

# DNS (port 53, tcp+udp)
uci add firewall rule # creer une nouvelle rule
uci set firewall.@rule[-1].name='Allow-DNS-iot' # on la nomme
uci set firewall.@rule[-1].src='iot'  # la source est la zone 'iot'
uci set firewall.@rule[-1].proto='tcp udp' # protocole tcp & udp
uci set firewall.@rule[-1].dest_port='53' # sur le port 53 (port par defaut DNS)
uci set firewall.@rule[-1].target='ACCEPT' # accepte sur la target

uci commit firewall # valide
/etc/init.d/firewall restart 
```

Pour tester nos regles, il suffit de tester et d'essayer de ping le reseau IotLab depuis LAN, on se rend compte que ca fonctionne. 

Mais quand on essaie rien que de se connecter en ssh sur le reseau IoTLab on voit que cela echoue (parce que la config n'a pas ete fate pour le port 22 et qu'il est donc fermee a cause de la regle globale qu'on a setup juste avant)


**Source(s):**
- *openwrt.org/docs/guide-user/firewall/firewall_configuration*