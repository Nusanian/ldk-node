//! Uji client LSPS2 — simulasi wallet yang minta JIT channel ke server LSP.
//!
//! Env:
//!   LSP_NODE_ID - node_id server LSP (hex pubkey)
//!   LSP_ADDR    - alamat P2P server, mis. "127.0.0.1:9738"
//!   LSP_TOKEN   - token akses (harus sama dengan server)
//!   CLIENT_STORAGE - direktori data client (default /tmp/lsp-client-test)
//!
//! Dijalankan: cargo run --example lsp_client_test
use ldk_node::bitcoin::Network;
use ldk_node::lightning::ln::msgs::SocketAddress;
use ldk_node::lightning_invoice::{Bolt11InvoiceDescription, Description};
use ldk_node::Builder;
use std::str::FromStr;
use std::time::Duration;

fn env(name: &str, default: &str) -> String {
	std::env::var(name).unwrap_or_else(|_| default.to_string())
}

fn main() {
	let lsp_node_id = env("LSP_NODE_ID", "");
	assert!(!lsp_node_id.is_empty(), "LSP_NODE_ID wajib diisi");
	let lsp_addr = env("LSP_ADDR", "127.0.0.1:9738");
	let token = env("LSP_TOKEN", "nux-lsp-test-token");
	let storage = env("CLIENT_STORAGE", "/tmp/lsp-client-test");
	let electrum = env("CLIENT_ELECTRUM", "163.245.193.102:60009");
	// Nominal invoice dalam msat (default 5.000 nusan = 5_000_000 msat)
	let amount_msat: u64 = std::env::var("INVOICE_MSAT")
		.ok()
		.and_then(|s| s.parse().ok())
		.unwrap_or(5_000_000);

	let node_id =
		ldk_node::bitcoin::secp256k1::PublicKey::from_str(&lsp_node_id).expect("node_id hex");
	let address = SocketAddress::from_str(&lsp_addr).expect("lsp addr");

	println!("[client] menghubungkan ke LSP {} @ {} ...", lsp_node_id, lsp_addr);
	let mut builder = Builder::new();
	builder.set_network(Network::NusacoinTestnet);
	builder.set_chain_source_electrum(electrum, None);
	builder.set_storage_dir_path(storage);
	builder.set_liquidity_source_lsps2(node_id, address, Some(token));

	let node = builder.build().expect("[client] build gagal");
	node.start().expect("[client] start gagal");
	println!("[client] node jalan, node_id={}", node.node_id());

	// Minta invoice JIT — ini memicu handshake LSPS2 GetInfo/Buy ke server.
	println!("[client] meminta invoice JIT {} msat...", amount_msat);
	let desc = Bolt11InvoiceDescription::Direct(Description::new("uji JIT NUX".to_string()).unwrap());
	match node.bolt11_payment().receive_via_jit_channel(amount_msat, &desc, 3600, None) {
		Ok(invoice) => {
			println!("[client] INVOICE OK: {}", invoice);
			println!("[client] handshake LSPS2 BERHASIL ✅");
		},
		Err(e) => {
			println!("[client] INVOICE GAGAL: {:?}", e);
			println!("[client] handshake LSPS2 GAGAL ❌");
		},
	}

	println!("[client] menunggu 30 detik untuk event...");
	for _ in 0..6 {
		std::thread::sleep(Duration::from_secs(5));
		while let Some(event) = node.next_event() {
			println!("[client] event: {:?}", event);
			node.event_handled().ok();
		}
	}

	// Mode tunggu: jika WAIT_PAYMENT=1, tetap online menunggu pembayaran JIT.
	// Ini mensimulasikan HP user yang standby menerima pembayaran.
	if std::env::var("WAIT_PAYMENT").ok().as_deref() == Some("1") {
		println!("[client] mode tunggu pembayaran JIT... (Ctrl+C untuk berhenti)");
		loop {
			let event = node.wait_next_event();
			println!("[client] event: {:?}", event);
			if node.event_handled().is_err() {
				break;
			}
		}
	}

	node.stop().ok();
	println!("[client] selesai.");
}
