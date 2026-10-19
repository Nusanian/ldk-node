//! Cetak address on-chain dari storage dir yang sudah ada.
//! Env: ADDR_STORAGE (default /tmp/lsp-server-testnet)
use ldk_node::bitcoin::Network;
use ldk_node::Builder;

fn main() {
	let storage =
		std::env::var("ADDR_STORAGE").unwrap_or_else(|_| "/tmp/lsp-server-testnet".to_string());
	let mut builder = Builder::new();
	builder.set_network(Network::NusacoinTestnet);
	// chain source dummy — tidak akan dipakai karena node tidak di-start
	builder.set_chain_source_electrum("127.0.0.1:1".to_string(), None);
	builder.set_storage_dir_path(storage);
	let node = builder.build().expect("build gagal");
	let addr = node.onchain_payment().new_address().expect("new_address gagal");
	println!("FUNDING_ADDRESS: {}", addr);
}
