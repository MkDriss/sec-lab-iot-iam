//! mqtt-sniffer — étape 1 : capture brute + hexdump.
//! Aucun parsing pour l'instant : on regarde les octets tels que libpcap les livre.

mod capture;

use clap::Parser;

/// Sniffer défensif MQTT (pédagogique).
#[derive(Parser, Debug)]
#[command(name = "mqtt-sniffer", version, about)]
struct Args {
    /// Interface à écouter (ex. lo, wlp3s0, eth0)
    #[arg(short, long, default_value = "lo")]
    interface: String,

    /// Filtre BPF exécuté dans le noyau (syntaxe tcpdump)
    #[arg(short, long, default_value = "tcp port 1883")]
    filter: String,

    /// Nombre de paquets à capturer avant de s'arrêter (0 = illimité)
    #[arg(short, long, default_value_t = 0)]
    count: u64,

    /// Liste les interfaces disponibles puis quitte
    #[arg(long)]
    list: bool,
}

/// Nombre d'octets affichés par paquet dans le hexdump.
/// 128 suffit pour voir Ethernet (14) + IPv4 (20) + TCP (20 à 60) + début du MQTT.
const PREVIEW_LEN: usize = 128;

fn main() -> Result<(), pcap::Error> {
    let args = Args::parse();

    if args.list {
        for dev in capture::list_interfaces()? {
            // `desc` est un Option<String> : as_deref() -> Option<&str>,
            // unwrap_or fournit une valeur par défaut si c'est None.
            println!("{:<16} {}", dev.name, dev.desc.as_deref().unwrap_or("-"));
        }
        return Ok(());
    }

    let mut cap = capture::open_live(&args.interface, &args.filter, 65535, 1000)?;

    // Type de couche liaison : 1 = DLT_EN10MB (Ethernet). Sur Linux, `lo`
    // fournit un faux en-tête Ethernet, donc on doit voir 1 ici aussi.
    println!(
        "[*] Écoute sur {} | filtre BPF : \"{}\" | linktype : {:?}",
        args.interface,
        args.filter,
        cap.get_datalink()
    );

    let mut seen: u64 = 0;
    loop {
        // next_packet() renvoie Result<Packet<'_>, pcap::Error>.
        // Le Packet EMPRUNTE le buffer interne de libpcap (zéro copie) :
        // il n'est valable que jusqu'au prochain appel à next_packet().
        match cap.next_packet() {
            Ok(packet) => {
                seen += 1;
                print_packet(seen, packet.header.len, packet.data);
                if args.count != 0 && seen >= args.count {
                    break;
                }
            }
            // Pas de paquet pendant `timeout_ms` : ce n'est pas une vraie erreur.
            Err(pcap::Error::TimeoutExpired) => continue,
            // Toute autre erreur est remontée à l'appelant (ici : fin du programme).
            Err(e) => return Err(e),
        }
    }

    Ok(())
}

/// Affiche un résumé + un hexdump (offset | hex | ASCII) du début du paquet.
/// `data: &[u8]` est une slice : une vue (pointeur + longueur) sur des octets
/// qui appartiennent à quelqu'un d'autre. Aucune copie n'est faite.
fn print_packet(n: u64, wire_len: u32, data: &[u8]) {
    println!("\n#{n:<5} {wire_len} octets sur le fil, {} capturés", data.len());

    // Sous-slice des PREVIEW_LEN premiers octets (sans dépasser la taille réelle).
    let preview = &data[..data.len().min(PREVIEW_LEN)];

    for (i, chunk) in preview.chunks(16).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
        let ascii: String = chunk
            .iter()
            .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' })
            .collect();
        println!("    {:04x}  {:<47}  {}", i * 16, hex.join(" "), ascii);
    }
}