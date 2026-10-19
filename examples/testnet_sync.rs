//! Uji sync ldk-node (fork X11 NUX) melawan electrs testnet tim.
//! Dijalankan: cargo run --example testnet_sync
use ldk_node::bitcoin::Network;
use ldk_node::Builder;
use std::time::Duration;

fn main() {
	println!("[test] membangun node (NusacoinTestnet, electrum 163.245.193.102:60009)...");
	let mut builder = Builder::new();
	builder.set_network(Network::NusacoinTestnet);
	builder.set_chain_source_electrum("163.245.193.102:60009".to_string(), None);
	builder.set_storage_dir_path("/tmp/ldk-nux-testnet-sync".to_string());

	let node = builder.build().expect("[test] build gagal");
	node.start().expect("[test] start gagal");
	println!("[test] node jalan, sync otomatis via electrum...");

	for i in 0..12 {
		std::thread::sleep(Duration::from_secs(10));
		let st = node.status();
		let bal = node.list_balances();
		println!(
			"[test] t+{}s running={} height={} hash={}... onchain_total={}",
			(i + 1) * 10,
			st.is_running,
			st.current_best_block.height,
			&st.current_best_block.block_hash.to_string()[..16],
			bal.total_onchain_balance_sats,
		);
	}

	println!("[test] SELESAI — menghentikan node.");
	node.stop().expect("[test] stop gagal");
}
