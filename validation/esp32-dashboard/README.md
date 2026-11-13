# ESP32-C3 Environment Dashboard — an EAK validation project

This is a complete small product: hardware and software, designed with EAK.

- **Hardware.** An ESP32-C3-WROOM-02 board, USB-C powered, with an SHT40 temperature/humidity sensor and an optional SSD1306 OLED.
- **Firmware.** Streams CRC-protected telemetry over USB.
- **Dashboard.** A host-side web dashboard.

The board was authored only through EAK's typed command engine (`eak_runtime::Engine::execute`). It was checked with EAK's ERC, DRC and MECH, and EAK's manufacturing compiler produced the outputs.

| What | Where |
|---|---|
| Requirements, architecture, component choices, pin map, PCB, thermal, mechanical, firmware | [`docs/DESIGN.md`](docs/DESIGN.md) |
| Evidence log (what was run, results, what is NOT VERIFIED) | [`READINESS.md`](READINESS.md) |
| Native EAK project (schematic + PCB + rules + library), command journal | [`project/`](project/) |
| EAK design driver (the commands that build the board) | [`design/src/lib.rs`](design/src/lib.rs), parts in [`design/src/parts.rs`](design/src/parts.rs) |
| ERC / DRC / MECH reports, engineering calculations, netlist, build log | [`outputs/reports/`](outputs/reports/) |
| Gerbers, Excellon drill, BOM, CPL (pick-and-place), SHA-256 manifest | [`outputs/manufacturing/`](outputs/manufacturing/) |
| Firmware workspace: `dashboard-core` (no_std), `dashboard-host` (dashboard), `esp32c3` (device binary) | [`firmware/`](firmware/) |

## Reproduce

You need a recent stable Rust (tested with 1.99.1 on Windows 11) and
`rustup target add riscv32imc-unknown-none-elf`. Nothing needs Python or ESP-IDF.

```sh
# 1. Rebuild the board through EAK, re-run ERC/DRC/MECH and the manufacturing compiler.
#    This rewrites project/ and outputs/.
cd validation/esp32-dashboard/design && cargo run --release

# 2. Design tests: EAK checks, package re-verification, committed project = fresh build,
#    firmware pin map vs the EAK netlist.
cargo test --release

# 3. Firmware logic and dashboard tests (host).
cd ../firmware && cargo test --release

# 4. Device firmware (ESP32-C3, esp-hal, no ESP-IDF).
cd esp32c3 && cargo build --release
#    Flash (needs hardware and espflash, not done in this project):
#    espflash flash --monitor target/riscv32imc-unknown-none-elf/release/esp32c3-dashboard-fw

# 5. Live dashboard. Simulated data is clearly labelled.
cd .. && cargo run --release --bin dashboard -- --simulate --faults
#    then open http://127.0.0.1:8080/
#    With the real board: cargo run --release --bin dashboard -- --serial \\.\COM5  (or /dev/ttyACM0)
```
