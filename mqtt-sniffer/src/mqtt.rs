//! Module `mqtt` : décodeur MQTT 3.1.1 écrit à partir de la spécification OASIS.
//!
//! Référence : *MQTT Version 3.1.1*, OASIS, 2014.
//! https://docs.oasis-open.org/mqtt/mqtt/v3.1.1/mqtt-v3.1.1.html
//! Les identifiants entre crochets (ex. `[MQTT-2.2.2-1]`) renvoient aux
//! exigences numérotées de la spécification.
//!
//! Principes de conception :
//! - **zéro-copie** : topics, payloads et identifiants sont des `&str` / `&[u8]`
//!   qui pointent dans le buffer d'entrée ;
//! - **fonction pure** : `decode` ne connaît ni le réseau ni TCP, elle prend des
//!   octets et dit ce qu'elle y trouve ;
//! - **framing explicite** : `decode` indique combien d'octets elle a consommés,
//!   ou combien il en manque, ce qui prépare le réassemblage TCP (étape 3b) ;
//! - **erreurs détaillées** : chaque violation de la spec a sa propre variante,
//!   car pour un IDS une anomalie est une information à rapporter.

use std::fmt::{self};

/// Port TCP standard de MQTT sans TLS.
pub const MQTT_PORT: u16 = 1883;

// ===========================================================================
// Types publics
// ===========================================================================

/// Les 14 types de paquets de contrôle (spec §2.2.1, table 2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketType {
    Connect,
    ConnAck,
    Publish,
    PubAck,
    PubRec,
    PubRel,
    PubComp,
    Subscribe,
    SubAck,
    Unsubscribe,
    UnsubAck,
    PingReq,
    PingResp,
    Disconnect,
}

impl PacketType {
    /// Convertit les 4 bits de poids fort du premier octet en type de paquet.
    /// Les valeurs 0 et 15 sont réservées (interdites) : on renvoie `None`.
    fn from_nibble(value: u8) -> Option<PacketType> {
        use PacketType::*;
        match value {
            1 => Some(Connect),
            2 => Some(ConnAck),
            3 => Some(Publish),
            4 => Some(PubAck),
            5 => Some(PubRec),
            6 => Some(PubRel),
            7 => Some(PubComp),
            8 => Some(Subscribe),
            9 => Some(SubAck),
            10 => Some(Unsubscribe),
            11 => Some(UnsubAck),
            12 => Some(PingReq),
            13 => Some(PingResp),
            14 => Some(Disconnect),
            _ => None,
        }
    }

    /// Valeur obligatoire des flags (bits 3-0) pour ce type (spec §2.2.2,
    /// table 2.2). PUBLISH est le seul type dont les flags varient : `None`.
    fn required_flags(self) -> Option<u8> {
        use PacketType::*;
        match self {
            Publish => None,
            PubRel | Subscribe | Unsubscribe => Some(0b0010),
            _ => Some(0b0000),
        }
    }

    /// Nom officiel du type, en majuscules comme dans la spec.
    pub fn name(self) -> &'static str {
        use PacketType::*;
        match self {
            Connect => "CONNECT",
            ConnAck => "CONNACK",
            Publish => "PUBLISH",
            PubAck => "PUBACK",
            PubRec => "PUBREC",
            PubRel => "PUBREL",
            PubComp => "PUBCOMP",
            Subscribe => "SUBSCRIBE",
            SubAck => "SUBACK",
            Unsubscribe => "UNSUBSCRIBE",
            UnsubAck => "UNSUBACK",
            PingReq => "PINGREQ",
            PingResp => "PINGRESP",
            Disconnect => "DISCONNECT",
        }
    }
}

/// Niveau de qualité de service (spec §4.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

impl QoS {
    /// 0, 1 ou 2 ; la valeur 3 est interdite [MQTT-3.3.1-4].
    fn from_u8(value: u8) -> Option<QoS> {
        match value {
            0 => Some(QoS::AtMostOnce),
            1 => Some(QoS::AtLeastOnce),
            2 => Some(QoS::ExactlyOnce),
            _ => None,
        }
    }
}

/// Message « testament » publié par le broker si le client disparaît
/// sans DISCONNECT (spec §3.1.2.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Will<'a> {
    pub topic: &'a str,
    pub message: &'a [u8],
    pub qos: QoS,
    pub retain: bool,
}

/// Contenu décodé d'un CONNECT (spec §3.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connect<'a> {
    pub protocol_level: u8,
    pub clean_session: bool,
    pub keep_alive: u16,
    pub client_id: &'a str,
    pub will: Option<Will<'a>>,
    pub username: Option<&'a str>,
    /// Données binaires (pas forcément du texte) [spec §3.1.3.5].
    pub password: Option<&'a [u8]>,
}

/// Contenu décodé d'un CONNACK (spec §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnAck {
    pub session_present: bool,
    pub return_code: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PubAck {
    pub packet_id: u16,
}

/// Contenu décodé d'un PUBLISH (spec §3.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publish<'a> {
    pub dup: bool,
    pub qos: QoS,
    pub retain: bool,
    pub topic: &'a str,
    /// Présent seulement si QoS > 0 [MQTT-2.3.1-5].
    pub packet_id: Option<u16>,
    pub payload: &'a [u8],
}

/// Contenu décodé d'un SUBSCRIBE (spec §3.8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscribe<'a> {
    pub packet_id: u16,
    /// Couples (filtre de topic, QoS demandée). Le `Vec` alloue de la place
    /// pour les couples, mais les chaînes elles-mêmes ne sont pas copiées.
    pub filters: Vec<(&'a str, QoS)>,
}

/// Contenu décodé d'un SUBACK (spec §3.9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubAck<'a> {
    pub packet_id: u16,
    /// Un code par filtre : 0x00, 0x01, 0x02 (QoS accordée) ou 0x80 (échec).
    pub return_codes: &'a [u8],
}

/// Un paquet MQTT décodé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet<'a> {
    Connect(Connect<'a>),
    ConnAck(ConnAck),
    Publish(Publish<'a>),
    Subscribe(Subscribe<'a>),
    SubAck(SubAck<'a>),
    PubAck(PubAck),
    PingReq,
    PingResp,
    Disconnect,
    /// Type reconnu mais pas encore décodé en détail (PUBACK, PUBREC, PUBREL,
    /// PUBCOMP, UNSUBSCRIBE, UNSUBACK). `body` = tout ce qui suit l'en-tête fixe.
    Other {
        packet_type: PacketType,
        body: &'a [u8],
    },
}

/// Résultat du décodage d'un buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decoded<'a> {
    /// Un paquet complet a été décodé ; il occupait les `consumed` premiers
    /// octets du buffer. Le paquet suivant, s'il existe, commence juste après.
    Complete { packet: Packet<'a>, consumed: usize },
    /// Le buffer contient le début d'un paquet, mais il manque au moins
    /// `needed` octets (ils arriveront dans un prochain segment TCP).
    Incomplete { needed: usize },
}

/// Toutes les violations de la spécification que le décodeur sait détecter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MqttError {
    /// Type 0 ou 15, réservés.
    InvalidPacketType(u8),
    /// Bits 3-0 de l'en-tête fixe différents de la valeur imposée.
    InvalidFlags { packet_type: PacketType, flags: u8 },
    /// Remaining Length codée sur plus de 4 octets.
    MalformedRemainingLength,
    /// Un champ annonce plus d'octets que le paquet n'en contient.
    BoundaryCrossed { field: &'static str },
    /// Le CONNECT ne commence pas par la chaîne "MQTT".
    InvalidProtocolName,
    /// Niveau de protocole autre que 4 (MQTT 3.1.1).
    UnsupportedProtocolLevel(u8),
    /// Combinaison de Connect Flags interdite.
    InvalidConnectFlags(&'static str),
    /// Bit réservé positionné à 1.
    ReservedBits { field: &'static str },
    /// QoS égale à 3.
    InvalidQos(u8),
    /// DUP à 1 sur un PUBLISH de QoS 0.
    DupWithQos0,
    /// Joker `+` ou `#` dans le topic d'un PUBLISH.
    WildcardInTopicName,
    /// Chaîne qui n'est pas de l'UTF-8 valide.
    InvalidUtf8 { field: &'static str },
    /// Caractère nul U+0000 dans une chaîne.
    NullCharacter { field: &'static str },
    /// Packet Identifier égal à 0.
    ZeroPacketId,
    /// SUBSCRIBE sans aucun filtre.
    EmptySubscribe,
    /// Code de retour de SUBACK hors de {0x00, 0x01, 0x02, 0x80}.
    InvalidSubAckCode(u8),
    /// Longueur différente de celle imposée pour ce type de paquet.
    UnexpectedLength {
        packet_type: PacketType,
        expected: usize,
        actual: usize,
    },
    /// Octets en trop après le dernier champ attendu.
    TrailingBytes {
        packet_type: PacketType,
        count: usize,
    },
}

impl fmt::Display for MqttError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MqttError::*;
        match self {
            InvalidPacketType(t) => write!(f, "type de paquet réservé ({t}) [MQTT-2.2.1]"),
            InvalidFlags { packet_type, flags } => write!(
                f,
                "flags {flags:#06b} invalides pour {} [MQTT-2.2.2-1]",
                packet_type.name()
            ),
            MalformedRemainingLength => {
                write!(f, "Remaining Length sur plus de 4 octets [MQTT-2.2.3]")
            }
            BoundaryCrossed { field } => {
                write!(f, "le champ « {field} » dépasse la fin du paquet")
            }
            InvalidProtocolName => {
                write!(f, "nom de protocole différent de \"MQTT\" [MQTT-3.1.2-1]")
            }
            UnsupportedProtocolLevel(l) => {
                write!(
                    f,
                    "niveau de protocole {l} non supporté (4 attendu) [MQTT-3.1.2-2]"
                )
            }
            InvalidConnectFlags(why) => write!(f, "Connect Flags invalides : {why}"),
            ReservedBits { field } => write!(f, "bit(s) réservé(s) à 1 dans « {field} »"),
            InvalidQos(q) => write!(f, "QoS {q} interdite [MQTT-3.3.1-4]"),
            DupWithQos0 => write!(f, "DUP à 1 sur un PUBLISH de QoS 0 [MQTT-3.3.1-2]"),
            WildcardInTopicName => {
                write!(f, "joker dans le topic d'un PUBLISH [MQTT-3.3.2-2]")
            }
            InvalidUtf8 { field } => {
                write!(f, "« {field} » n'est pas de l'UTF-8 valide [MQTT-1.5.3-1]")
            }
            NullCharacter { field } => write!(f, "caractère nul dans « {field} » [MQTT-1.5.3-2]"),
            ZeroPacketId => write!(f, "Packet Identifier nul [MQTT-2.3.1-1]"),
            EmptySubscribe => write!(f, "SUBSCRIBE sans filtre [MQTT-3.8.3-3]"),
            InvalidSubAckCode(c) => {
                write!(f, "code de retour SUBACK {c:#04x} réservé [MQTT-3.9.3-2]")
            }
            UnexpectedLength {
                packet_type,
                expected,
                actual,
            } => write!(
                f,
                "{} de longueur {actual} (attendu : {expected})",
                packet_type.name()
            ),
            TrailingBytes { packet_type, count } => {
                write!(
                    f,
                    "{count} octet(s) en trop à la fin d'un {}",
                    packet_type.name()
                )
            }
        }
    }
}

impl std::error::Error for MqttError {}

// ===========================================================================
// Point d'entrée : decode
// ===========================================================================

/// Décode le paquet MQTT qui commence au début de `buf`.
///
/// - `Ok(Decoded::Complete { packet, consumed })` : un paquet complet ;
/// - `Ok(Decoded::Incomplete { needed })` : début de paquet, données manquantes ;
/// - `Err(..)` : les octets violent la spécification.
pub fn decode(buf: &[u8]) -> Result<Decoded<'_>, MqttError> {
    // `let ... else` : si le motif ne correspond pas, on exécute le bloc `else`,
    // qui doit obligatoirement quitter la fonction (ici avec `return`).
    let Some(&byte1) = buf.first() else {
        // Un paquet fait au minimum 2 octets (type + Remaining Length).
        return Ok(Decoded::Incomplete { needed: 2 });
    };

    // --- En-tête fixe, octet 1 : type (bits 7-4) et flags (bits 3-0) ---
    let type_nibble = byte1 >> 4;
    let packet_type =
        PacketType::from_nibble(type_nibble).ok_or(MqttError::InvalidPacketType(type_nibble))?;
    let flags = byte1 & 0x0F;
    if let Some(required) = packet_type.required_flags() {
        if flags != required {
            return Err(MqttError::InvalidFlags { packet_type, flags });
        }
    }

    // --- En-tête fixe, octets 2 à 5 : Remaining Length ---
    let Some((remaining_len, len_len)) = decode_remaining_length(&buf[1..])? else {
        return Ok(Decoded::Incomplete { needed: 1 });
    };

    // --- Framing : a-t-on reçu tout le paquet ? ---
    let header_len = 1 + len_len;
    let total_len = header_len + remaining_len;
    if buf.len() < total_len {
        return Ok(Decoded::Incomplete {
            needed: total_len - buf.len(),
        });
    }
    let body = &buf[header_len..total_len];

    // --- Décodage spécifique à chaque type ---
    let packet = match packet_type {
        PacketType::Connect => Packet::Connect(decode_connect(body)?),
        PacketType::ConnAck => Packet::ConnAck(decode_connack(body)?),
        PacketType::Publish => Packet::Publish(decode_publish(flags, body)?),
        PacketType::Subscribe => Packet::Subscribe(decode_subscribe(body)?),
        PacketType::SubAck => Packet::SubAck(decode_suback(body)?),
        PacketType::PubAck => Packet::PubAck(decode_puback(body)?),
        PacketType::PingReq => {
            expect_len(packet_type, body, 0)?;
            Packet::PingReq
        }
        PacketType::PingResp => {
            expect_len(packet_type, body, 0)?;
            Packet::PingResp
        }
        PacketType::Disconnect => {
            expect_len(packet_type, body, 0)?;
            Packet::Disconnect
        }
        other => Packet::Other {
            packet_type: other,
            body,
        },
    };

    Ok(Decoded::Complete {
        packet,
        consumed: total_len,
    })
}

/// Décode la Remaining Length (spec §2.2.3) à partir de l'octet 2 du paquet.
///
/// Renvoie `Ok(Some((valeur, nombre_d_octets_utilisés)))`, ou `Ok(None)` si les
/// octets de la longueur ne sont pas encore tous arrivés.
///
/// Chaque octet porte 7 bits de valeur ; le bit de poids fort (0x80) signifie
/// « un autre octet suit ». Au plus 4 octets sont autorisés.
fn decode_remaining_length(bytes: &[u8]) -> Result<Option<(usize, usize)>, MqttError> {
    let mut value: usize = 0;
    let mut multiplier: usize = 1;
    for (index, &encoded) in bytes.iter().enumerate().take(4) {
        value += usize::from(encoded & 0x7F) * multiplier;
        if encoded & 0x80 == 0 {
            return Ok(Some((value, index + 1)));
        }
        multiplier *= 128;
    }
    if bytes.len() >= 4 {
        // 4 octets lus et le 4e annonçait encore une suite : interdit.
        Err(MqttError::MalformedRemainingLength)
    } else {
        Ok(None)
    }
}

// ===========================================================================
// Lecteur séquentiel (curseur) sur le corps d'un paquet
// ===========================================================================

/// Lit des champs les uns après les autres dans une slice, en vérifiant à
/// chaque fois qu'on ne dépasse pas la fin. `'a` est la durée de vie des
/// octets lus : les slices renvoyées pointent dans le buffer d'origine.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(buf: &'a [u8]) -> Self {
        Reader { buf, pos: 0 }
    }

    /// Nombre d'octets pas encore lus.
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Lit exactement `n` octets.
    fn bytes(&mut self, n: usize, field: &'static str) -> Result<&'a [u8], MqttError> {
        if n > self.remaining() {
            return Err(MqttError::BoundaryCrossed { field });
        }
        let buf = self.buf; // copie de la référence (pas des octets)
        let slice = &buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn u8(&mut self, field: &'static str) -> Result<u8, MqttError> {
        Ok(self.bytes(1, field)?[0])
    }

    /// Entier 16 bits big-endian (octet de poids fort en premier) [spec §1.5.2].
    fn u16(&mut self, field: &'static str) -> Result<u16, MqttError> {
        let b = self.bytes(2, field)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    /// Données binaires préfixées par leur longueur sur 2 octets [spec §1.5.3].
    fn binary(&mut self, field: &'static str) -> Result<&'a [u8], MqttError> {
        let len = usize::from(self.u16(field)?);
        self.bytes(len, field)
    }

    /// Chaîne UTF-8 préfixée par sa longueur sur 2 octets [spec §1.5.3].
    fn string(&mut self, field: &'static str) -> Result<&'a str, MqttError> {
        let raw = self.binary(field)?;
        // `from_utf8` vérifie l'encodage SANS copier : la `&str` renvoyée
        // pointe sur les mêmes octets que `raw`.
        let text = std::str::from_utf8(raw).map_err(|_| MqttError::InvalidUtf8 { field })?;
        if text.contains('\0') {
            return Err(MqttError::NullCharacter { field });
        }
        Ok(text)
    }

    /// Tout ce qui reste (utilisé pour le payload d'un PUBLISH).
    fn rest(&mut self) -> &'a [u8] {
        let buf = self.buf;
        let slice = &buf[self.pos..];
        self.pos = buf.len();
        slice
    }

    /// Vérifie qu'il ne reste aucun octet non lu.
    fn expect_end(&self, packet_type: PacketType) -> Result<(), MqttError> {
        match self.remaining() {
            0 => Ok(()),
            count => Err(MqttError::TrailingBytes { packet_type, count }),
        }
    }
}

/// Vérifie qu'un corps de paquet a exactement la longueur imposée.
fn expect_len(packet_type: PacketType, body: &[u8], expected: usize) -> Result<(), MqttError> {
    if body.len() == expected {
        Ok(())
    } else {
        Err(MqttError::UnexpectedLength {
            packet_type,
            expected,
            actual: body.len(),
        })
    }
}

// ===========================================================================
// Décodage de chaque type de paquet
// ===========================================================================

/// CONNECT (spec §3.1).
fn decode_connect(body: &[u8]) -> Result<Connect<'_>, MqttError> {
    let mut r = Reader::new(body);

    // En-tête variable : nom du protocole, niveau, flags, keep alive.
    if r.binary("nom du protocole")? != b"MQTT" {
        return Err(MqttError::InvalidProtocolName);
    }
    let protocol_level = r.u8("niveau de protocole")?;
    if protocol_level != 4 {
        return Err(MqttError::UnsupportedProtocolLevel(protocol_level));
    }

    let flags = r.u8("connect flags")?;
    if flags & 0b0000_0001 != 0 {
        return Err(MqttError::ReservedBits {
            field: "connect flags [MQTT-3.1.2-3]",
        });
    }
    let clean_session = flags & 0b0000_0010 != 0;
    let will_flag = flags & 0b0000_0100 != 0;
    let will_qos = (flags >> 3) & 0b11;
    let will_retain = flags & 0b0010_0000 != 0;
    let password_flag = flags & 0b0100_0000 != 0;
    let username_flag = flags & 0b1000_0000 != 0;

    if !will_flag && (will_qos != 0 || will_retain) {
        return Err(MqttError::InvalidConnectFlags(
            "Will QoS ou Will Retain non nuls sans Will Flag [MQTT-3.1.2-13] [MQTT-3.1.2-15]",
        ));
    }
    if password_flag && !username_flag {
        return Err(MqttError::InvalidConnectFlags(
            "Password Flag sans User Name Flag [MQTT-3.1.2-22]",
        ));
    }

    let keep_alive = r.u16("keep alive")?;

    // Payload : les champs apparaissent dans cet ordre précis [MQTT-3.1.3-1].
    let client_id = r.string("client identifier")?;
    if client_id.is_empty() && !clean_session {
        return Err(MqttError::InvalidConnectFlags(
            "ClientId vide avec Clean Session à 0 [MQTT-3.1.3-7]",
        ));
    }

    let will = if will_flag {
        let qos = QoS::from_u8(will_qos).ok_or(MqttError::InvalidQos(will_qos))?;
        Some(Will {
            topic: r.string("will topic")?,
            message: r.binary("will message")?,
            qos,
            retain: will_retain,
        })
    } else {
        None
    };
    let username = if username_flag {
        Some(r.string("user name")?)
    } else {
        None
    };
    let password = if password_flag {
        Some(r.binary("password")?)
    } else {
        None
    };

    r.expect_end(PacketType::Connect)?;

    Ok(Connect {
        protocol_level,
        clean_session,
        keep_alive,
        client_id,
        will,
        username,
        password,
    })
}

/// CONNACK (spec §3.2) : toujours exactement 2 octets.
fn decode_connack(body: &[u8]) -> Result<ConnAck, MqttError> {
    expect_len(PacketType::ConnAck, body, 2)?;
    if body[0] & 0b1111_1110 != 0 {
        return Err(MqttError::ReservedBits {
            field: "connect acknowledge flags",
        });
    }
    Ok(ConnAck {
        session_present: body[0] & 0b0000_0001 != 0,
        return_code: body[1],
    })
}

/// PUBLISH (spec §3.3). Les flags de l'en-tête fixe portent DUP, QoS, RETAIN.
fn decode_publish(flags: u8, body: &[u8]) -> Result<Publish<'_>, MqttError> {
    let dup = flags & 0b1000 != 0;
    let qos_bits = (flags >> 1) & 0b11;
    let retain = flags & 0b0001 != 0;

    let qos = QoS::from_u8(qos_bits).ok_or(MqttError::InvalidQos(qos_bits))?;
    if qos == QoS::AtMostOnce && dup {
        return Err(MqttError::DupWithQos0);
    }

    let mut r = Reader::new(body);
    let topic = r.string("topic name")?;
    if topic.contains(['+', '#']) {
        return Err(MqttError::WildcardInTopicName);
    }

    let packet_id = if qos == QoS::AtMostOnce {
        None
    } else {
        let id = r.u16("packet identifier")?;
        if id == 0 {
            return Err(MqttError::ZeroPacketId);
        }
        Some(id)
    };

    Ok(Publish {
        dup,
        qos,
        retain,
        topic,
        packet_id,
        payload: r.rest(),
    })
}

/// SUBSCRIBE (spec §3.8) : Packet Identifier puis une liste (filtre, QoS).
fn decode_subscribe(body: &[u8]) -> Result<Subscribe<'_>, MqttError> {
    let mut r = Reader::new(body);
    let packet_id = r.u16("packet identifier")?;
    if packet_id == 0 {
        return Err(MqttError::ZeroPacketId);
    }

    let mut filters = Vec::new();
    while r.remaining() > 0 {
        let filter = r.string("topic filter")?;
        let requested = r.u8("requested qos")?;
        if requested & 0b1111_1100 != 0 {
            return Err(MqttError::ReservedBits {
                field: "requested qos [MQTT-3-8.3-4]",
            });
        }
        let qos = QoS::from_u8(requested).ok_or(MqttError::InvalidQos(requested))?;
        filters.push((filter, qos));
    }

    if filters.is_empty() {
        return Err(MqttError::EmptySubscribe);
    }
    Ok(Subscribe { packet_id, filters })
}

/// SUBACK (spec §3.9) : Packet Identifier puis un code par filtre.
fn decode_suback(body: &[u8]) -> Result<SubAck<'_>, MqttError> {
    let mut r = Reader::new(body);
    let packet_id = r.u16("packet identifier")?;
    let return_codes = r.rest();
    if let Some(&bad) = return_codes
        .iter()
        .find(|&&code| !matches!(code, 0x00 | 0x01 | 0x02 | 0x80))
    {
        return Err(MqttError::InvalidSubAckCode(bad));
    }
    Ok(SubAck {
        packet_id,
        return_codes,
    })
}

fn decode_puback(body: &[u8]) -> Result<PubAck, MqttError> {
    expect_len(PacketType::PubAck, body, 2)?;
    let packet_id: u16 = u16::from_be_bytes([body[0], body[1]]);
    Ok(PubAck { packet_id })
}

// ===========================================================================
// Affichage lisible
// ===========================================================================

/// Affiche des octets comme texte s'ils sont imprimables, sinon en hexadécimal.
fn show_bytes(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) if !text.chars().any(char::is_control) => format!("\"{text}\""),
        _ => {
            let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
            format!("0x{}", hex.join(""))
        }
    }
}

impl Packet<'_> {
    /// Résumé d'une ligne. Les mots de passe sont masqués sauf si
    /// `show_secrets` vaut `true` (un outil de sécurité ne doit pas exposer de secrets par défaut)
    pub fn describe(&self, show_secrets: bool) -> String {
        match self {
            Packet::Connect(c) => {
                let client_id = if c.client_id.is_empty() {
                    "\"\" (vide : identifiant attribué par le broker)".to_string()
                } else {
                    format!("\"{}\"", c.client_id)
                };
                let user = c.username.map_or("-".to_string(), |u| format!("\"{u}\""));
                let pass = match (c.password, show_secrets) {
                    (None, _) => "-".to_string(),
                    (Some(p), true) => show_bytes(p),
                    (Some(p), false) => format!("<masqué, {} octets>", p.len()),
                };
                let will = c
                    .will
                    .as_ref()
                    .map_or("-".to_string(), |w| format!("\"{}\"", w.topic));
                format!(
                    "CONNECT client_id={client_id} clean_session={} keep_alive={}s \
                     user={user} pass={pass} will={will}",
                    u8::from(c.clean_session),
                    c.keep_alive
                )
            }
            Packet::ConnAck(a) => {
                let meaning = match a.return_code {
                    0 => "accepté",
                    1 => "refusé : version de protocole",
                    2 => "refusé : identifiant rejeté",
                    3 => "refusé : serveur indisponible",
                    4 => "refusé : identifiants invalides",
                    5 => "refusé : non autorisé",
                    _ => "code réservé",
                };
                format!(
                    "CONNACK code={} ({meaning}) session_present={}",
                    a.return_code,
                    u8::from(a.session_present)
                )
            }
            Packet::PubAck(p) => {
                format!("PUBACK id={}", p.packet_id)
            }
            Packet::Publish(p) => {
                let id = p.packet_id.map_or("-".to_string(), |id| id.to_string());
                format!(
                    "PUBLISH topic=\"{}\" qos={} retain={} dup={} id={id} payload={}",
                    p.topic,
                    p.qos as u8,
                    u8::from(p.retain),
                    u8::from(p.dup),
                    show_bytes(p.payload)
                )
            }
            Packet::Subscribe(s) => {
                let list: Vec<String> = s
                    .filters
                    .iter()
                    .map(|(filter, qos)| format!("\"{filter}\"(qos {})", *qos as u8))
                    .collect();
                format!("SUBSCRIBE id={} filtres=[{}]", s.packet_id, list.join(", "))
            }
            Packet::SubAck(s) => {
                let codes: Vec<String> =
                    s.return_codes.iter().map(|c| format!("{c:#04x}")).collect();
                format!("SUBACK id={} codes=[{}]", s.packet_id, codes.join(", "))
            }
            Packet::PingReq => "PINGREQ".to_string(),
            Packet::PingResp => "PINGRESP".to_string(),
            Packet::Disconnect => "DISCONNECT".to_string(),
            Packet::Other { packet_type, body } => {
                format!("{} ({} octets, non décodé)", packet_type.name(), body.len())
            }
        }
    }
}

// ===========================================================================
// Tests unitaires
// ===========================================================================
#[cfg(test)]
mod tests {

    use super::*;

    /// Petite aide : décode un paquet complet ou fait échouer le test.
    fn complete(buf: &[u8]) -> (Packet<'_>, usize) {
        match decode(buf) {
            Ok(Decoded::Complete { packet, consumed }) => (packet, consumed),
            other => panic!("paquet complet attendu, obtenu : {other:?}"),
        }
    }

    // --- Octets réels capturés sur le labo (étape 2) ---

    #[test]
    fn connect_de_mosquitto_sub() {
        let buf = [
            0x10, 0x0c, 0x00, 0x04, b'M', b'Q', b'T', b'T', 0x04, 0x02, 0x00, 0x3c, 0x00, 0x00,
        ];
        let (packet, consumed) = complete(&buf);
        assert_eq!(consumed, 14);
        let Packet::Connect(c) = packet else {
            panic!("CONNECT attendu")
        };
        assert_eq!(c.protocol_level, 4);
        assert!(c.clean_session);
        assert_eq!(c.keep_alive, 60);
        assert_eq!(c.client_id, "");
        assert_eq!(c.username, None);
        assert_eq!(c.password, None);
    }

    #[test]
    fn connack_accepte() {
        let (packet, _) = complete(&[0x20, 0x02, 0x00, 0x00]);
        assert_eq!(
            packet,
            Packet::ConnAck(ConnAck {
                session_present: false,
                return_code: 0
            })
        );
    }

    #[test]
    fn puback_accepte() {
        let (packet, _) = complete(&[0x40, 0x02, 0x00, 0x01]);
        assert_eq!(packet, Packet::PubAck(PubAck { packet_id: 1 }));
    }

    #[test]
    fn subscribe_labo_diese() {
        let buf = [
            0x82, 0x0b, 0x00, 0x01, 0x00, 0x06, b'l', b'a', b'b', b'o', b'/', b'#', 0x00,
        ];
        let (packet, _) = complete(&buf);
        let Packet::Subscribe(s) = packet else {
            panic!("SUBSCRIBE attendu")
        };
        assert_eq!(s.packet_id, 1);
        assert_eq!(s.filters, vec![("labo/#", QoS::AtMostOnce)]);
    }

    #[test]
    fn suback_qos0() {
        let (packet, _) = complete(&[0x90, 0x03, 0x00, 0x01, 0x00]);
        let Packet::SubAck(s) = packet else {
            panic!("SUBACK attendu")
        };
        assert_eq!(s.packet_id, 1);
        assert_eq!(s.return_codes, &[0x00]);
    }

    #[test]
    fn publish_temperature() {
        let buf = [
            0x30, 0x0f, 0x00, 0x09, b'l', b'a', b'b', b'o', b'/', b't', b'e', b'm', b'p', b'2',
            b'1', b'.', b'5',
        ];
        let (packet, consumed) = complete(&buf);
        assert_eq!(consumed, 17);
        let Packet::Publish(p) = packet else {
            panic!("PUBLISH attendu")
        };
        assert_eq!(p.topic, "labo/temp");
        assert_eq!(p.qos, QoS::AtMostOnce);
        assert_eq!(p.packet_id, None);
        assert_eq!(p.payload, b"21.5");
    }

    #[test]
    fn ping_et_disconnect() {
        assert_eq!(complete(&[0xc0, 0x00]).0, Packet::PingReq);
        assert_eq!(complete(&[0xd0, 0x00]).0, Packet::PingResp);
        assert_eq!(complete(&[0xe0, 0x00]).0, Packet::Disconnect);
    }

    // --- Framing ---

    #[test]
    fn deux_paquets_dans_un_meme_buffer() {
        // PINGREQ puis DISCONNECT collés : le premier décodage consomme 2 octets.
        let buf = [0xc0, 0x00, 0xe0, 0x00];
        let (first, consumed) = complete(&buf);
        assert_eq!((first, consumed), (Packet::PingReq, 2));
        assert_eq!(complete(&buf[consumed..]).0, Packet::Disconnect);
    }

    #[test]
    fn paquet_coupe_est_incomplet() {
        // PUBLISH de 17 octets dont seuls les 10 premiers sont arrivés.
        let buf = [0x30, 0x0f, 0x00, 0x09, b'l', b'a', b'b', b'o', b'/', b't'];
        assert_eq!(decode(&buf), Ok(Decoded::Incomplete { needed: 7 }));
        assert_eq!(decode(&[]), Ok(Decoded::Incomplete { needed: 2 }));
        assert_eq!(decode(&[0x30]), Ok(Decoded::Incomplete { needed: 1 }));
    }

    #[test]
    fn remaining_length_sur_deux_octets() {
        // Exemple de la spec §2.2.3 : 321 = 65 + 2*128, codé 0xC1 0x02.
        assert_eq!(decode_remaining_length(&[0xc1, 0x02]), Ok(Some((321, 2))));
        assert_eq!(decode_remaining_length(&[0x40]), Ok(Some((64, 1))));
        assert_eq!(
            decode_remaining_length(&[0xff, 0xff, 0xff, 0x7f]),
            Ok(Some((268_435_455, 4)))
        );
    }

    // --- Anomalies (ce que l'IDS devra signaler) ---

    #[test]
    fn remaining_length_sur_cinq_octets_est_malformee() {
        let buf = [0x30, 0xff, 0xff, 0xff, 0xff, 0x7f];
        assert_eq!(decode(&buf), Err(MqttError::MalformedRemainingLength));
    }

    #[test]
    fn subscribe_avec_mauvais_flags() {
        // 0x80 au lieu de 0x82 : les bits réservés doivent valoir 0010.
        let buf = [0x80, 0x05, 0x00, 0x01, 0x00, 0x01, b'a'];
        assert_eq!(
            decode(&buf),
            Err(MqttError::InvalidFlags {
                packet_type: PacketType::Subscribe,
                flags: 0
            })
        );
    }

    #[test]
    fn type_reserve_est_rejete() {
        assert_eq!(decode(&[0xf0, 0x00]), Err(MqttError::InvalidPacketType(15)));
    }

    #[test]
    fn longueur_de_topic_mensongere() {
        // Le topic annonce 200 octets alors que le paquet n'en contient que 2.
        let buf = [0x30, 0x04, 0x00, 0xc8, b'a', b'b'];
        assert_eq!(
            decode(&buf),
            Err(MqttError::BoundaryCrossed {
                field: "topic name"
            })
        );
    }

    #[test]
    fn publish_qos3_est_interdit() {
        let buf = [0x36, 0x03, 0x00, 0x01, b'a'];
        assert_eq!(decode(&buf), Err(MqttError::InvalidQos(3)));
    }

    #[test]
    fn puback_de_mauvaise_longueur_est_rejete() {
        let packet = [0x40, 0x03, 0x00, 0x01, 0x01];
        assert_eq!(
            decode(&packet),
            Err(MqttError::UnexpectedLength {
                packet_type: PacketType::PubAck,
                expected: 2,
                actual: 3
            })
        );
    }
    // --- Identifiants en clair : le cœur du problème IAM ---

    /// CONNECT d'un objet avec identifiant, nom d'utilisateur et mot de passe.
    const CONNECT_AVEC_IDENTIFIANTS: [u8; 37] = [
        0x10, 0x23, // CONNECT, Remaining Length = 35
        0x00, 0x04, b'M', b'Q', b'T', b'T', 0x04, // "MQTT", niveau 4
        0xc2, // flags : user + password + clean session
        0x00, 0x3c, // keep alive 60 s
        0x00, 0x0a, b'a', b'c', b't', b'i', b'o', b'n', b'n', b'e', b'u', b'r', // client id
        0x00, 0x05, b'a', b'd', b'm', b'i', b'n', // user name
        0x00, 0x04, b'1', b'2', b'3', b'4', // password
    ];

    #[test]
    fn connect_avec_identifiants_en_clair() {
        let (packet, _) = complete(&CONNECT_AVEC_IDENTIFIANTS);
        let Packet::Connect(c) = &packet else {
            panic!("CONNECT attendu")
        };
        assert_eq!(c.client_id, "actionneur");
        assert_eq!(c.username, Some("admin"));
        assert_eq!(c.password, Some(&b"1234"[..]));

        // Le mot de passe est masqué par défaut, visible seulement sur demande.
        assert!(packet.describe(false).contains("pass=<masqué, 4 octets>"));
        assert!(packet.describe(true).contains("pass=\"1234\""));
    }

    #[test]
    fn password_sans_username_est_interdit() {
        let mut buf = CONNECT_AVEC_IDENTIFIANTS;
        buf[9] = 0x42; // password flag sans user name flag
        assert!(matches!(
            decode(&buf),
            Err(MqttError::InvalidConnectFlags(_))
        ));
    }
}
