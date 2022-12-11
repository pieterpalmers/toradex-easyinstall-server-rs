// image server
#[macro_use]
extern crate rouille;

use if_addrs::{IfAddr, Ifv4Addr};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{io, thread};

use rouille::{Response, Server};

use std::sync::mpsc;

use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{arg, command, value_parser};

pub fn start_mdns(service_hostname: &String, port: &u16) -> ServiceDaemon {
    let mut service_hostname = service_hostname.clone();
    service_hostname.push_str(".local");

    // Create a new mDNS daemon.
    let mdns = ServiceDaemon::new().expect("Could not create service daemon");
    let mut service_type = String::from("_tezi._tcp");
    service_type.push_str(".local.");
    let instance_name = "PentaSono Toradex local feed";
    let ifaces = my_ipv4_interfaces();
    dbg!(&ifaces);
    let my_addrs: Vec<Ipv4Addr> = ifaces.iter().map(|i| i.ip).collect();
    dbg!(&my_addrs);

    let properties = HashMap::<String, String>::from([
        (
            "name".to_string(),
            "Toradex EasyInstaller Local PentaSono Feed".to_string(),
        ),
        ("path".to_string(), "/image_list.json".to_string()),
        ("enabled".to_string(), "1".to_string()),
        ("https".to_string(), "0".to_string()),
    ]);

    // Register a service.
    let service_info = ServiceInfo::new(
        &service_type,
        &instance_name,
        &service_hostname,
        &my_addrs[..],
        *port,
        Some(properties),
    )
    .expect("valid service info");

    mdns.register(service_info)
        .expect("Failed to register mDNS service");

    println!(
        "Registered mDNS service {}.{}",
        &instance_name, &service_type
    );

    mdns
}

fn my_ipv4_interfaces() -> Vec<Ifv4Addr> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| {
            if i.is_loopback() {
                None
            } else {
                match i.addr {
                    IfAddr::V4(ifv4) => Some(ifv4),
                    _ => None,
                }
            }
        })
        .collect()
}

pub fn start_server(basedir: &String, port: &u16) -> (thread::JoinHandle<()>, mpsc::Sender<()>) {
    let basedir = basedir.clone();
    let server = Server::new(SocketAddr::from(([0, 0, 0, 0], *port)), move |request| {
        rouille::log(&request, io::stdout(), || {
            rouille::match_assets(&request, &basedir)
        })
    })
    .unwrap();

    println!("Listening on {:?}", server.server_addr());
    let (h, s) = server.stoppable();
    (h, s)
}

fn main() {
    let matches = command!() // requires `cargo` feature
        .arg(arg!(<base_dir> "base directory containing image_list.json"))
        .arg(
            arg!(--port [port] "port to serve on")
                .value_parser(value_parser!(u16))
                .default_value("8123"),
        )
        .get_matches();

    let hostname = gethostname::gethostname().into_string().unwrap();
    let abortflag: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

    // shutdown handler
    println!("registering SIGINT handler...");
    let abortflag_ = abortflag.clone();
    ctrlc::set_handler(move || {
        println!("Shutting down...");
        abortflag_.swap(true, Ordering::Relaxed);
    })
    .expect("unable to set SIGINT handler");

    let port = matches.get_one::<u16>("port").unwrap_or(&8123);
    let base_dir = matches.get_one::<String>("base_dir").unwrap();

    let mdns = start_mdns(&hostname, port);

    let (h_webserver, c_webserver) = start_server(base_dir, port);

    while !abortflag.load(Ordering::Relaxed) {
        thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("Shutting down...");

    // request the webserver to stop
    c_webserver.send(()).unwrap();

    mdns.shutdown().unwrap();

    // join the webserver thread
    h_webserver.join().unwrap();

    println!("Bye");
}
