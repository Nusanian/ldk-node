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

	let node_id =
		ldk_node::bitcoin::secp256k1::PublicKey::from_str(&lsp_node_id).expect("node_id hex");
	let address = SocketAddress::from_str(&lsp_addr).expect("lsp addr");

	println!("[client] menghubungkan ke LSP {} @ {} ...", lsp_node_id, lsp_addr);
	let mut builder = Builder::new();
	builder.set_network(Network::NusacoinTestnet);
	builder.set_chain_source_electrum("163.245.193.102:60009".to_string(), None);
	builder.set_storage_dir_path(storage);
	builder.set_liquidity_source_lsps2(node_id, address, Some(token));

	let node = builder.build().expect("[client] build gagal");
	node.start().expect("[client] start gagal");
	println!("[client] node jalan, node_id={}", node.node_id());

	// Minta invoice JIT — ini memicu handshake LSPS2 GetInfo/Buy ke server.
	println!("[client] meminta invoice JIT 100.000 nusan (100_000_000 msat)...");
	let desc = Bolt11InvoiceDescription::Direct(Description::new("uji JIT NUX".to_string()).unwrap());
	match node.bolt11_payment().receive_via_jit_channel(100_000_000, &desc, 3600, None) {
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

	node.stop().ok();
	println!("[client] selesai.");
}
