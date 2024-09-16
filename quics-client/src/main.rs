use std::net::ToSocketAddrs;

use clap::Parser;
use quics_client::connect::connection::Builder as ConnectionBuilder;
use quics_client::connect::stream::Builder as StreamBuilder;
use quics_client::socks::Socks5;
use quics_protocol::client::Client;

/// QUICS Client
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Remote server IP address or domain name. e.g. example.com:port
    #[arg(short, long)]
    remote: String,

    /// SOCKS server listening address
    #[arg(short, long, default_value = "127.0.0.1:1080")]
    listen: String,

    /// IO provider address for the client
    #[arg(long, default_value = "0.0.0.0:0")]
    bind: String,

    /// TLS SNI, if not provided, remote address will be used
    #[arg(long, default_value = None)]
    tls_sni: Option<String>,

    /// TLS certificate file path
    #[arg(long, default_value = None)]
    tls_cert: Option<String>,

    /// Initial congestion window size in bytes
    #[arg(long, default_value = None)]
    initial_congestion_window: Option<u32>,

    /// e.g. INFO WARN ERROR
    #[arg(long, default_value = "WARN")]
    tracing_level: tracing::Level,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    #[cfg(feature = "trace")]
    tracing_subscriber::fmt()
        .with_thread_ids(true)
        .with_max_level(args.tracing_level)
        .init();

    let client = {
        use std::path::Path;

        use s2n_quic::provider::congestion_controller;
        use s2n_quic::Client as NoiseClient;

        let controller = {
            let controller = congestion_controller::bbr::Builder::default();
            let controller = match args.initial_congestion_window {
                Some(value) => controller.with_initial_congestion_window(value),
                None => controller,
            };
            controller.build()
        };

        let client = NoiseClient::builder()
            .with_io(args.bind.as_str())?
            .with_congestion_controller(controller)?;

        match args.tls_cert {
            Some(path) => client.with_tls(Path::new(path.as_str()))?.start()?,
            None => client.start()?,
        }
    };

    let server_name = match args.tls_sni {
        Some(value) => value,
        None => {
            let address = args.remote.clone();
            let pos = address.rfind(':').ok_or("invalid remote address")?;
            address[..pos].to_string()
        }
    };

    let server_address = args
        .remote
        .to_socket_addrs()?
        .nth(0)
        .ok_or(format!("unable to resolve address {}", args.remote))?;

    let connection = ConnectionBuilder::new(client, server_name, server_address).build();
    let stream = StreamBuilder::new(connection).build();

    let socks_server = Socks5::with(args.listen).await?;

    Client::with(socks_server, stream).start().await;

    Ok(())
}
