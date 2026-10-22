use clap::Parser;
use std::net::TcpStream;

/// Scanner de ports TCP simple (inspiré de nmap)
#[derive(Parser)]
#[command(name = "scanner", about = "Scanner de ports TCP")]

struct Args {
    /// Adresse IP ou hôte à scanner
    #[arg(short = 't', long = "target", default_value = "127.0.0.1")]
    target: String,

    /// Port de début
    #[arg(short = 'p', long = "port-start", default_value_t = 1)]
    port_start: u16,

    /// Port de fin
    #[arg(short = 'P', long = "port-end", default_value_t = 1024)]
    port_end: u16,
}

fn main() {
    let args = Args::parse();

    println!(
        "Scan de {} sur les ports {}..={}",
        args.target, args.port_start, args.port_end
    );

    for port in args.port_start..=args.port_end {
        match TcpStream::connect((args.target.as_str(), port)) {
            Ok(_) => println!("Port {port} : OUVERT"),
            Err(_) => println!("Port {port} : fermé"),
        }
    }
}