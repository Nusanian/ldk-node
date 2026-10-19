//! Node pengirim untuk uji JIT full-loop.
//!
//! Env:
//!   SENDER_STORAGE - default /tmp/jit-sender (boleh /tmp, tidak ada dana)
//!   SENDER_LISTEN  - default 127.0.0.1:9739
//!   PAY_INVOICE    - jika diisi, bayar invoice ini lalu tunggu 60 dtk
//!
//! Tanpa PAY_INVOICE: hanya jalan, cetak node_id, tunggu.
use ldk_node::bitcoin::Network;
use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::Builder;
use std::str::FromStr;
use std::time::Duration;

fn env(name: &str, default: &str) -> String {
	std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn main() {
	let storage = env("SENDER_STORAGE", "/tmp/jit-sender");
	let listen = env("SENDER_LISTEN", "127.0.0.1:9739");
	let pay_invoice = std::env::var("PAY_INVOICE").ok();

	let mut builder = Builder::new();
	builder.set_network(Network::NusacoinTestnet);
	builder.set_chain_source_electrum("163.245.193.102:60009".to_string(), None);
	builder.set_storage_dir_path(storage);
	builder
		.set_listening_addresses(vec![SocketAddress::from_str(&listen).unwrap()])
		.unwrap();

	let node = builder.build().expect("[sender] build gagal");
	node.start().expect("[sender] start gagal");
	println!("[sender] node_id={} listen={}", node.node_id(), listen);

	// Opsi: sambung ke peer saat startup (format node_id@host:port).
	// Berguna untuk reconnect setelah restart — LDK tidak auto-reconnect.
	if let Ok(conn) = std::env::var("CONNECT_PEER") {
		let parts: Vec<&str> = conn.split('@').collect();
		if parts.len() == 2 {
			let peer_id = ldk_node::bitcoin::secp256k1::PublicKey::from_str(parts[0])
				.expect("CONNECT_PEER node_id tidak valid");
			let addr = SocketAddress::from_str(parts[1]).expect("CONNECT_PEER addr tidak valid");
			match node.connect(peer_id, addr, true) {
				Ok(_) => println!("[sender] terhubung ke peer {}", peer_id),
				Err(e) => println!("[sender] GAGAL connect: {:?}", e),
			}
		}
	}

	if let Some(inv_str) = pay_invoice {
		println!("[sender] membayar invoice...");
		let invoice: ldk_node::lightning_invoice::Bolt11Invoice =
			inv_str.parse().expect("invoice tidak valid");
		match node.bolt11_payment().send(&invoice, None) {
			Ok(pid) => println!("[sender] payment dikirim, id={:?}", pid),
			Err(e) => println!("[sender] GAGAL bayar: {:?}", e),
		}
		// Tunggu hasil pembayaran
		for _ in 0..12 {
			std::thread::sleep(Duration::from_secs(5));
			while let Some(event) = node.next_event() {
				println!("[sender] event: {:?}", event);
				node.event_handled().ok();
			}
		}
	} else {
		println!("[sender] menunggu channel dari LSP... (Ctrl+C untuk berhenti)");
		loop {
			let event = node.wait_next_event();
			println!("[sender] event: {:?}", event);
			if node.event_handled().is_err() {
				break;
			}
		}
	}
	node.stop().ok();
}
