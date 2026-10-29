//! Node pengirim untuk uji JIT full-loop.
//!
//! Env:
//!   SENDER_STORAGE - default /tmp/jit-sender (boleh /tmp, tidak ada dana)
//!   SENDER_LISTEN  - default 127.0.0.1:9739
//!   SENDER_ELECTRUM - alamat electrum, mis. "163.245.193.102:60009"
//!   SENDER_NETWORK  - "mainnet" | "testnet" (default testnet)
//!   SENDER_SEED_PATH - path file seed (opsional, untuk node_id persisten)
//!   PAY_INVOICE    - jika diisi, bayar invoice ini lalu tunggu 60 dtk
//!   CREATE_INVOICE_MSAT - jika diisi, buat invoice sebesar ini (mode terima /
//!                    top-up rig): tunggu channel usable, cetak saldo channel
//!                    SEBELUM, buat invoice, tunggu PaymentReceived, cetak
//!                    saldo SESUDAH. Invoice juga ditulis ke INVOICE_OUT_FILE
//!                    bila env itu diisi.
//!   DUMP_CHANNELS  - jika diisi, cetak saldo channel berkala di mode tunggu.
//!
//! Tanpa PAY_INVOICE: hanya jalan, cetak node_id, tunggu.
use ldk_node::bitcoin::Network;
use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::lightning_invoice::{Bolt11InvoiceDescription, Description};
use ldk_node::Builder;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn env(name: &str, default: &str) -> String {
	std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn dump_channels(node: &ldk_node::Node, tag: &str) {
	let chans = node.list_channels();
	if chans.is_empty() {
		println!("[sender] channels[{}]: (tidak ada channel)", tag);
	}
	for ch in chans {
		println!(
			"[sender] channels[{}]: id={:?} value={} nusan usable={} outbound={} msat inbound={} msat",
			tag, ch.channel_id, ch.channel_value_sats, ch.is_usable,
			ch.outbound_capacity_msat, ch.inbound_capacity_msat
		);
	}
}

fn main() {
	let storage = env("SENDER_STORAGE", "/tmp/jit-sender");
	let listen = env("SENDER_LISTEN", "127.0.0.1:9739");
	let electrum = env("SENDER_ELECTRUM", "163.245.193.102:60009");
	let seed_path = std::env::var("SENDER_SEED_PATH").ok();
	let pay_invoice = std::env::var("PAY_INVOICE").ok();

	let mut builder = Builder::new();
	let network_name = env("SENDER_NETWORK", "testnet");
	let network =
		if network_name == "mainnet" { Network::Nusacoin } else { Network::NusacoinTestnet };
	println!("[sender] network={}", network_name);
	builder.set_network(network);
	builder.set_chain_source_electrum(electrum, None);
	builder.set_storage_dir_path(storage);
	if let Some(sp) = seed_path {
		println!("[sender] memakai seed file: {}", sp);
		builder.set_entropy_seed_path(sp);
	}
	builder
		.set_listening_addresses(vec![SocketAddress::from_str(&listen).unwrap()])
		.unwrap();

	let node = builder.build().expect("[sender] build gagal");
	node.start().expect("[sender] start gagal");
	println!("[sender] node_id={} listen={}", node.node_id(), listen);

	// Signal handler: SIGTERM/SIGINT → shutdown graceful via node.stop().
	// Tanpa ini, pkill langsung bunuh proses dan wallet korup (WalletSetupFailed).
	let running = Arc::new(AtomicBool::new(true));
	let r = running.clone();
	ctrlc::set_handler(move || {
		println!("\n[sender] sinyal diterima, shutdown graceful...");
		r.store(false, Ordering::SeqCst);
	});

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

	let create_msat = std::env::var("CREATE_INVOICE_MSAT")
		.ok()
		.and_then(|v| v.parse::<u64>().ok());
	if let Some(amount_msat) = create_msat {
		// Mode terima (top-up): tunggu channel usable dengan inbound cukup.
		println!("[sender] mode CREATE_INVOICE: menunggu channel usable (inbound >= {} msat)...", amount_msat);
		let mut ready = false;
		for _ in 0..45 {
			if !running.load(Ordering::SeqCst) {
				break;
			}
			for ch in node.list_channels() {
				if ch.is_usable && ch.inbound_capacity_msat >= amount_msat {
					ready = true;
				}
			}
			if ready {
				break;
			}
			std::thread::sleep(Duration::from_secs(2));
		}
		if !ready {
			println!("[sender] GAGAL: tidak ada channel usable dengan inbound cukup setelah 90 dtk");
		} else {
			dump_channels(&node, "SEBELUM-TOPUP");
			let desc = Bolt11InvoiceDescription::Direct(
				Description::new("topup sender rig".to_string()).unwrap(),
			);
			match node.bolt11_payment().receive(amount_msat, &desc, 3600) {
				Ok(invoice) => {
					let inv_str = invoice.to_string();
					println!("[sender] INVOICE: {}", inv_str);
					if let Ok(path) = std::env::var("INVOICE_OUT_FILE") {
						let _ = std::fs::write(&path, &inv_str);
						println!("[sender] invoice ditulis ke {}", path);
					}
					println!("[sender] menunggu pembayaran masuk (maks ~3 menit)...");
					let mut received = false;
					for _ in 0..36 {
						if received || !running.load(Ordering::SeqCst) {
							break;
						}
						std::thread::sleep(Duration::from_secs(5));
						while let Some(event) = node.next_event() {
							println!("[sender] event: {:?}", event);
							if matches!(event, ldk_node::Event::PaymentReceived { .. }) {
								received = true;
							}
							node.event_handled().ok();
						}
					}
					if received {
						// Beri jeda sebentar agar saldo channel final terbaca.
						std::thread::sleep(Duration::from_secs(3));
						dump_channels(&node, "SESUDAH-TOPUP");
					} else {
						println!("[sender] pembayaran TIDAK diterima dalam batas waktu");
						dump_channels(&node, "SESUDAH-TOPUP-GAGAL");
					}
				},
				Err(e) => println!("[sender] GAGAL buat invoice: {:?}", e),
			}
		}
	} else if let Some(inv_str) = pay_invoice {
		let invoice: ldk_node::lightning_invoice::Bolt11Invoice =
			inv_str.parse().expect("invoice tidak valid");
		let need_msat = invoice.amount_milli_satoshis().unwrap_or(0);

		// TUNGGU channel usable dulu, BARU bayar. Setelah reconnect,
		// channel_reestablish butuh beberapa detik sampai channel
		// berstatus usable; membayar sebelum itu selalu RouteNotFound
		// (akar kegagalan pembayaran berulang 19-20 Okt 2026).
		println!("[sender] menunggu channel usable (perlu >= {} msat)...", need_msat);
		let mut usable = false;
		for i in 0..45 {
			if !running.load(Ordering::SeqCst) {
				break;
			}
			for ch in node.list_channels() {
				println!(
					"[sender] channel {:?} usable={} outbound={} msat",
					ch.channel_id, ch.is_usable, ch.outbound_capacity_msat
				);
				if ch.is_usable && ch.outbound_capacity_msat >= need_msat {
					usable = true;
				}
			}
			if usable {
				println!("[sender] channel usable setelah ~{} dtk", i * 2);
				break;
			}
			std::thread::sleep(Duration::from_secs(2));
		}
		if !usable {
			println!("[sender] PERINGATAN: belum ada channel usable setelah 90 dtk - tetap coba bayar");
		}

		println!("[sender] membayar invoice...");
		match node.bolt11_payment().send(&invoice, None) {
			Ok(pid) => println!("[sender] payment dikirim, id={:?}", pid),
			Err(e) => println!("[sender] GAGAL bayar: {:?}", e),
		}
		// Tunggu hasil pembayaran sampai final (sukses/gagal) atau
		// maksimal ~3 menit - buka channel JIT + forward HTLC bisa
		// makan waktu; jangan matikan node di tengah pembayaran.
		let mut done = false;
		for _ in 0..36 {
			if done || !running.load(Ordering::SeqCst) {
				break;
			}
			std::thread::sleep(Duration::from_secs(5));
			while let Some(event) = node.next_event() {
				println!("[sender] event: {:?}", event);
				if matches!(
					event,
					ldk_node::Event::PaymentSuccessful { .. }
						| ldk_node::Event::PaymentFailed { .. }
				) {
					done = true;
				}
				node.event_handled().ok();
			}
		}
	} else {
		println!("[sender] menunggu channel dari LSP... (Ctrl+C untuk berhenti)");
		let dump = std::env::var("DUMP_CHANNELS").is_ok();
		let mut tick: u64 = 0;
		while running.load(Ordering::SeqCst) {
			// Poll dengan timeout agar bisa cek sinyal shutdown.
			// wait_next_event() blokir selamanya dan tidak bisa diinterupsi.
			if let Some(event) = node.next_event() {
				println!("[sender] event: {:?}", event);
				if node.event_handled().is_err() {
					break;
				}
			} else {
				std::thread::sleep(Duration::from_millis(500));
				tick += 1;
				if dump && tick % 20 == 0 {
					dump_channels(&node, "berkala");
				}
			}
		}
	}
	println!("[sender] shutdown...");
	node.stop().ok();
	println!("[sender] berhenti dengan bersih.");
}
