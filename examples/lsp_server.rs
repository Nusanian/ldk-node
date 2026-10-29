//! Server LSP Nusacoin — layani JIT channel via LSPS2 (bLIP-52).
//!
//! Konfigurasi via environment variable:
//!   LSP_NETWORK    - "testnet" (default) atau "mainnet"
//!   LSP_STOP_FILE  - path file sinyal stop graceful (default "/tmp/stop-lsp";
//!                    wajib beda per instance bila dua server jalan di satu mesin)
//!   LSP_ELECTRUM   - alamat electrum, mis. "163.245.193.102:60009"
//!   LSP_LISTEN     - alamat listen P2P, mis. "0.0.0.0:9738"
//!   LSP_STORAGE    - direktori data, mis. "/tmp/lsp-server-testnet"
//!   LSP_TOKEN      - token akses wajib dari client (require_token)
//!   LSP_PAY_FILE   - path file pemicu bayar langsung: operator menulis
//!                    invoice (satu baris) ke file ini, server membayarnya
//!                    tepat satu kali lalu mengosongkan file. Untuk kirim
//!                    langsung LSP -> HP tanpa perantara sender.
//!   LSP_PAY_MAX_NUSAN - PENGAMAN WAJIB mode bayar: batas maksimum nominal
//!                    per pembayaran, dalam nusan. Bila LSP_PAY_FILE terisi
//!                    tapi env ini kosong/tidak valid, mode bayar NONAKTIF
//!                    total (server tetap jalan, tidak membayar apa pun).
//!                    Invoice tanpa nominal atau di atas batas ditolak.
//!   LSP_PAY_ONLY_NODE - PENGAMAN opsional: node_id hex (66 karakter) satu-
//!                    satunya tujuan yang boleh dibayar. Bila terisi tapi
//!                    tidak valid, mode bayar juga NONAKTIF total.
//!   LSP_PROMISE_SECRET - hex 32 byte untuk promise_secret LSPS2
//!   LSP_ALIAS      - alias node (opsional, default "NUX-LSP-testnet")
//!
//! Dijalankan: cargo run --example lsp_server
use ldk_node::bitcoin::Network;
use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::liquidity::LSPS2ServiceConfig;
use ldk_node::Builder;
use std::str::FromStr;
use std::time::Duration;

fn env(name: &str, default: &str) -> String {
	std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn main() {
	let network_name = env("LSP_NETWORK", "testnet");
	let is_mainnet = network_name == "mainnet";
	let network = if is_mainnet { Network::Nusacoin } else { Network::NusacoinTestnet };
	let electrum = env("LSP_ELECTRUM", "163.245.193.102:60009");
	let listen = env("LSP_LISTEN", "0.0.0.0:9738");
	let storage = env("LSP_STORAGE", "/tmp/lsp-server-testnet");
	let seed_path = std::env::var("LSP_SEED_PATH").ok();
	let token = env("LSP_TOKEN", "nux-lsp-test-token");
	let alias = env("LSP_ALIAS", if is_mainnet { "NUX-LSP-mainnet" } else { "NUX-LSP-testnet" });
	let promise_hex =
		env("LSP_PROMISE_SECRET", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
	let mut promise_secret = [0u8; 32];
	for (i, chunk) in promise_hex.as_bytes().chunks(2).enumerate().take(32) {
		let s = std::str::from_utf8(chunk).unwrap();
		promise_secret[i] = u8::from_str_radix(s, 16).unwrap();
	}

	println!("[lsp] network={} electrum={} listen={} storage={}", network_name, electrum, listen, storage);

	let service_config = LSPS2ServiceConfig {
		require_token: Some(token),
		advertise_service: true,
		channel_opening_fee_ppm: 5_000, // 0.5% — keputusan NUX 18 Okt 2026
		channel_over_provisioning_ppm: 100_000, // 10%
		min_channel_opening_fee_msat: 1_000, // 1 nusan — keputusan NUX
		min_channel_lifetime: 36, // ~54 jam @ 90 mnt/block
		max_client_to_self_delay: 2016,
		min_payment_size_msat: 1_000_000, // 1.000 nusan
		max_payment_size_msat: 5_000_000_000, // 5.000.000 nusan
		client_trusts_lsp: true,
	};

	let mut builder = Builder::new();
	builder.set_network(network);
	builder.set_chain_source_electrum(electrum, None);
	builder.set_storage_dir_path(storage);
	if let Some(sp) = seed_path {
		println!("[lsp] memakai seed file: {}", sp);
		builder.set_entropy_seed_path(sp);
	}
	builder
		.set_listening_addresses(vec![SocketAddress::from_str(&listen).expect("listen addr")])
		.expect("set_listening_addresses");
	builder.set_node_alias(alias).expect("set_node_alias");
	builder.set_liquidity_provider_lsps2(service_config);

	let node = builder.build().expect("[lsp] build gagal");
	let fund_addr = node.onchain_payment().new_address().expect("[lsp] new_address gagal");
	println!("[lsp] FUNDING_ADDRESS: {}", fund_addr);
	// node_id dicetak SEBELUM start: start() bisa gagal bila electrum belum
	// terjangkau (mis. electrs masih sync), tapi identitas node sudah final
	// sejak build() dan dibutuhkan untuk konfigurasi client.
	println!("[lsp] node_id={}", node.node_id());
	node.start().expect("[lsp] start gagal");
	println!("[lsp] melayani LSPS2, Ctrl+C untuk berhenti");

	// Uji: buka channel ke peer jika diminta via LSP_PEER_OPEN=node_id@host:port:sats:push_msat
	// Tunggu wallet sync dulu (maks 5 menit) agar dana terlihat.
	if let Ok(spec) = std::env::var("LSP_PEER_OPEN") {
		println!("[lsp] menunggu wallet sync sebelum buka channel...");
		let mut synced_balance = 0u64;
		for _ in 0..60 {
			std::thread::sleep(Duration::from_secs(5));
			let bal = node.list_balances();
			if bal.total_onchain_balance_sats > 0 {
				synced_balance = bal.total_onchain_balance_sats;
				break;
			}
		}
		println!("[lsp] saldo on-chain: {} sats", synced_balance);
		let parts: Vec<&str> = spec.split('@').collect();
		let peer_id =
			ldk_node::bitcoin::secp256k1::PublicKey::from_str(parts[0]).expect("peer node_id");
		let ap: Vec<&str> = parts[1].split(':').collect();
		let addr = SocketAddress::from_str(&format!("{}:{}", ap[0], ap[1])).expect("peer addr");
		let sats: u64 = ap[2].parse().expect("sats");
		let push_msat: u64 = ap[3].parse().expect("push_msat");
		println!("[lsp] membuka channel ke {} ({} sats, push {} msat)...", peer_id, sats, push_msat);
		match node.open_channel(peer_id, addr, sats, Some(push_msat), None) {
			Ok(id) => println!("[lsp] channel open diminta, id={:?}", id),
			Err(e) => println!("[lsp] GAGAL open channel: {:?}", e),
		}
	}

	// Mode bayar langsung: file pemicu (operator menulis invoice ke file).
	// Pengaman: tanpa LSP_PAY_MAX_NUSAN yang valid mode bayar NONAKTIF
	// total — tidak ada keadaan "mode bayar hidup tanpa pagar".
	let pay_file = std::env::var("LSP_PAY_FILE").ok();
	let pay_max_nusan: Option<u64> = std::env::var("LSP_PAY_MAX_NUSAN")
		.ok()
		.and_then(|v| v.trim().parse::<u64>().ok())
		.filter(|v| *v > 0);
	let pay_only_node_raw = std::env::var("LSP_PAY_ONLY_NODE").ok();
	let pay_only_node: Option<String> = pay_only_node_raw
		.as_ref()
		.map(|v| v.trim().to_lowercase())
		.filter(|v| v.len() == 66 && v.chars().all(|c| c.is_ascii_hexdigit()));
	let pay_config_valid = pay_max_nusan.is_some()
		&& (pay_only_node_raw.is_none() || pay_only_node.is_some());
	let pay_active = pay_file.is_some() && pay_config_valid;
	if let Some(pf) = &pay_file {
		if pay_active {
			println!(
				"[lsp] mode bayar langsung AKTIF: tulis invoice ke {} (pengaman: batas {} nusan/pembayaran{})",
				pf,
				pay_max_nusan.unwrap(),
				match &pay_only_node {
					Some(n) => format!(", hanya ke node {}", n),
					None => ", tanpa pembatasan tujuan".to_string(),
				}
			);
		} else {
			println!(
				"[lsp] PERINGATAN: mode bayar NONAKTIF — LSP_PAY_MAX_NUSAN wajib diisi (dan LSP_PAY_ONLY_NODE harus hex 66 karakter bila diisi). Server tetap jalan tapi TIDAK akan membayar invoice apa pun."
			);
		}
	}

	// Event loop: laporkan event penting, jalan sampai ada file stop
	// (graceful shutdown agar wallet tidak korup — JANGAN kill -9)
	let stop_file = env("LSP_STOP_FILE", "/tmp/stop-lsp");
	println!("[lsp] graceful stop: sentuh {}", stop_file);
	let _ = std::fs::remove_file(&stop_file);
	// Diagnostik rig: cetak saldo channel berkala bila LSP_DUMP_CHANNELS diisi.
	let dump_channels = std::env::var("LSP_DUMP_CHANNELS").is_ok();
	let mut tick: u64 = 0;
	loop {
		tick += 1;
		if dump_channels && tick % 100 == 1 {
			for ch in node.list_channels() {
				println!(
					"[lsp] channels[berkala]: id={:?} value={} nusan usable={} outbound={} msat inbound={} msat",
					ch.channel_id, ch.channel_value_sats, ch.is_usable,
					ch.outbound_capacity_msat, ch.inbound_capacity_msat
				);
			}
		}
		if std::path::Path::new(&stop_file).exists() {
			println!("[lsp] stop signal diterima, shutdown graceful...");
			break;
		}
		// Pemicu bayar langsung: ada invoice baru di file pemicu?
		if let Some(pf) = &pay_file {
			if let Ok(content) = std::fs::read_to_string(pf) {
				let invoice_str = content.trim().to_string();
				if !invoice_str.is_empty() {
					// Kosongkan dulu: satu invoice dibayar tepat satu kali,
					// walau proses bayarnya sendiri ternyata gagal.
					let _ = std::fs::write(pf, "");
					match invoice_str
						.parse::<ldk_node::lightning_invoice::Bolt11Invoice>()
					{
						Ok(invoice) => {
							// Pengaman SEBELUM membayar: mode nonaktif,
							// invoice tanpa nominal, nominal di atas batas,
							// atau tujuan di luar whitelist selalu ditolak.
							let amount_nusan = invoice
								.amount_milli_satoshis()
								.map(|m| m / 1000);
							let mut tolak: Option<String> = None;
							if !pay_active {
								tolak = Some(
									"mode bayar nonaktif (konfigurasi pengaman tidak lengkap)"
										.to_string(),
								);
							} else if amount_nusan.is_none() {
								tolak = Some("invoice tanpa nominal".to_string());
							} else if amount_nusan.unwrap()
								> pay_max_nusan.unwrap()
							{
								tolak = Some(format!(
									"nominal {} nusan melebihi batas LSP_PAY_MAX_NUSAN {} nusan",
									amount_nusan.unwrap(),
									pay_max_nusan.unwrap()
								));
							} else if let Some(only) = &pay_only_node {
								let payee =
									invoice.recover_payee_pub_key().to_string();
								if payee != *only {
									tolak = Some(format!(
										"tujuan {} di luar LSP_PAY_ONLY_NODE {}",
										payee, only
									));
								}
							}
							match tolak {
								Some(alasan) => println!(
									"[lsp] pembayaran DITOLAK (pengaman): {}",
									alasan
								),
								None => {
									println!(
										"[lsp] membayar invoice langsung ({} nusan)...",
										amount_nusan.unwrap_or(0)
									);
									match node.bolt11_payment().send(&invoice, None) {
										Ok(pid) => println!(
											"[lsp] pembayaran langsung terkirim, id={:?}",
											pid
										),
										Err(e) => println!(
											"[lsp] pembayaran langsung GAGAL: {:?}",
											e
										),
									}
								}
							}
						}
						Err(e) => println!(
							"[lsp] invoice di file bayar tidak valid: {:?}",
							e
						),
					}
				}
			}
		}
		// wait_next_event dengan timeout via polling
		let mut got_event = false;
		for _ in 0..20 {
			if let Some(event) = node.next_event() {
				println!("[lsp] event: {:?}", event);
				node.event_handled().ok();
				got_event = true;
				break;
			}
			std::thread::sleep(Duration::from_millis(100));
		}
		if !got_event {
			std::thread::sleep(Duration::from_millis(100));
		}
	}

	node.stop().ok();
}
