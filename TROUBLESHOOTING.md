# Troubleshooting ldk-node (Nusacoin)

Catatan masalah yang ditemui saat uji LSP/JIT di testnet Nusacoin (Okt 2026).
Ditulis dari pengalaman langsung, bukan teori.

## 1. BDK wallet korup setiap restart (KRITIS)

**Gejala:**
```
[lsp] build gagal: WalletSetupFailed
ERROR [ldk_node::io::utils] Failed to deserialize BDK wallet field: Nonsense bytes didn't map to the type they were interpreted as
```

**Penyebab:** Setiap proses ldk-node berhenti tidak clean (SIGTERM, SIGKILL, VM replace/restart),
data BDK wallet di sqlite korup saat dibaca ulang. Terjadi juga SATU KALI setelah graceful shutdown.

**Workaround (terbukti):**
```bash
rm -rf <STORAGE_DIR> && mkdir -p <STORAGE_DIR>
# seed file DIPERTAHANKAN, mis. ~/workspace/lsp-test/seed/lsp-seed.bin
# start ulang -> node_id sama (dari seed), saldo on-chain ketemu lagi via resync
```

**Catatan:**
- Saldo on-chain selalu pulih (address derivasi dari seed).
- State channel (ChannelManager/ChannelMonitor) HILANG. Dana di channel yang masih terbuka
  terkunci di 2-of-2 multisig — hanya bisa dipulihkan kalau peer masih ada untuk force-close.
- Jangan pernah andalkan channel state selamat dari restart di environment ini.

**Pelajaran keras:** JANGAN PERNAH taruh wallet/seed di /tmp (hilang tiap VM replace).
Insiden 18 Okt 2026: 500rb nusan testnet hangus permanen karena wallet di /tmp.

## 2. ChannelReady tidak muncul padahal funding sudah confirmed

**Gejala:** `ChannelPending` diterima kedua sisi, funding tx confirmed di chain,
tapi event `ChannelReady` tidak pernah muncul (ditunggu 40+ menit).

**Root cause:** Koneksi P2P antar node putus. Di `channel.rs`:
```rust
if self.context.channel_state.is_peer_disconnected() {
    log_debug!(logger, "Not producing channel_ready: the peer is disconnected.");
    return None;
}
```
LDK menolak membuat `channel_ready` kalau peer disconnect. Log ini level DEBUG
sehingga tidak kelihatan tanpa debug logging — cek `ldk_node.log` di storage dir.

**Cara diagnosa:**
```bash
grep -i "channel_ready" <storage>/ldk_node.log          # 0 hasil = tidak pernah dicoba
grep -i "confirmed in block" <storage>/ldk_node.log     # funding terlihat?
grep -i "noise handshake" <storage>/ldk_node.log | tail # koneksi terakhir kapan?
```

**Solusi:** Reconnect manual. ldk-node TIDAK auto-reconnect. Contoh di `jit_sender.rs`
ditambah env `CONNECT_PEER=node_id@host:port` untuk connect saat startup.

## 3. InsufficientFunds padahal saldo cukup

**Gejala:** Saldo on-chain mis. 28.876 sats, tapi `open_channel` 10.000 sats gagal
dengan `InsufficientFunds`.

**Penyebab:** Saldo terfragmentasi di banyak change address kecil (hasil 4x buka
channel semalam), atau change masih UNCONFIRMED (dari funding tx yang belum confirmed).
Sisa confirmed yang lain bisa jadi sudah TERKUNCI di channel-channel zombie dari sesi
sebelumnya. Coin selection ldk-node butuh UTXO confirmed yang cukup besar.

**Solusi:** Minta top-up fresh ke funding address supaya ada UTXO utuh yang confirmed.
Jangan andalkan sisa receh dari banyak channel open.
Kasus nyata 19 Okt 2026: top-up 100rb nusan ke `tn1quszw8y0ztr2p52genmezk7998g5v93a8mdzgvv`
(1 confirmed) → channel baru langsung bisa dibuka.

## 4. electrs tim lambat / mempool view basi

**Gejala:**
- Query `blockchain.scripthash.*` kadang timeout (>25 detik).
- `server.version` selalu cepat, tapi batch query lambat (~3 detik).
- electrs REST `/mempool/recent` menampilkan tx yang SUDAH confirmed sebagai mempool
  (padahal `nusacoind -testnet getmempoolinfo` size 0). → Bug di fork electrs,
  sudah dilaporkan ke Melkisedek03.

**Dampak:**
- `FEE_RATE_CACHE_UPDATE_TIMEOUT_SECS` default (5 dtk) terlalu ketat → panic
  `FeerateEstimationUpdateTimeout` saat start. Sudah di-patch jadi 30 dtk di
  `ldk-node/src/config.rs` (penyesuaian lingkungan uji, belum tentu untuk produksi).
- Funding tx yang di-broadcast via electrs BISA TIDAK SAMPAI ke nusacoind.
  Workaround: ambil raw tx via `blockchain.transaction.get`, broadcast manual:
  ```
  nusacoin-cli -testnet sendrawtransaction <hex>
  ```

## 5. VM replace berkala

VM Strider di-replace tiap ~1-2 jam tanpa peringatan. Akibat:
- Semua proses mati. /tmp di-wipe total.
- BDK wallet hampir pasti korup (lihat #1).

**Aturan:** Semua yang persisten (seed, wallet dir, binary, script) WAJIB di ~/workspace.
Proses node hanya dijalankan saat dibutuhkan (ada block baru / uji aktif),
jangan dibiarkan idle berjam-jam.

## 6. Fee estimation di chain sepi

`estimatefee` di testnet yang sepi bisa mengembalikan nilai aneh atau lambat.
Untuk uji, fee kecil (1-2 sat/vbyte) cukup karena tidak ada kompetisi block.

## Checklist sebelum uji

- [ ] Seed di `~/workspace` (+ backup), BUKAN /tmp
- [ ] Binary `lsp_server`/`jit_sender` sudah build dari source terbaru
- [ ] electrs (163.245.193.102:60009) merespons `server.version`
- [ ] Tip chain diketahui (bandingkan electrum vs REST API vs `getblockcount`)
- [ ] Server punya UTXO confirmed yang cukup untuk channel yang mau dibuka
- [ ] Setelah buka channel: verifikasi funding tx masuk mempool nusacoind
      (`nusacoin-cli -testnet getmempoolinfo`), JANGAN hanya percaya electrs
