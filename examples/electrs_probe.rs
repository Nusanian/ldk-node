//! Probe diagnostik: ukur latency tiap method Electrum lawan electrs testnet tim.
//! Dijalankan: cargo run --example electrs_probe
use electrum_client::{Client, ElectrumApi};
use std::time::Instant;

fn main() {
	let url = "tcp://163.245.193.102:60009";
	println!("[probe] konek ke {} ...", url);
	let t = Instant::now();
	let client = match Client::new(url) {
		Ok(c) => {
			println!("[probe] konek OK dalam {:?}", t.elapsed());
			c
		}
		Err(e) => {
			println!("[probe] GAGAL konek: {}", e);
			return;
		}
	};

	let t = Instant::now();
	match client.block_headers_subscribe() {
		Ok(h) => println!(
			"[probe] headers.subscribe OK {:?}: height={} hash={}...",
			t.elapsed(),
			h.height,
			&h.header.block_hash().to_string()[..16]
		),
		Err(e) => println!("[probe] headers.subscribe GAGAL {:?}: {}", t.elapsed(), e),
	}

	let t = Instant::now();
	match client.estimate_fee(6) {
		Ok(f) => println!("[probe] estimatefee(6) OK {:?}: {}", t.elapsed(), f),
		Err(e) => println!("[probe] estimatefee(6) GAGAL {:?}: {}", t.elapsed(), e),
	}

	println!("[probe] SELESAI");
}
