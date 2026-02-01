# Citadel Terminal (GUI) Walkthrough

This guide explains how to launch the graphical interface for the Sovereign Citadel Protocol.

## Prerequisites
- Node.js (npm) installed.
- Rust toolchain installed.

## Setup
1. Navigate to the UI directory:
   ```bash
   cd crates/scp-ui
   ```
2. Install dependencies:
   ```bash
   npm install
   ```

## Running the Terminal
Start the app in development mode (easiest for testing):
```bash
npm run tauri dev
```
OR build the production executable:
```bash
npm run tauri build
```

## Features
- **Dashboard**: View real-time sentiment analysis (simulated).
- **Governance**: Stake your tokens and claim rewards using the backend logic.
- **Vault**: View system metrics (TVL, Circuit Breaker).

## Troubleshooting

### Windows File Locking (os error 32)
If you encounter build errors related to file access (e.g., `os error 32`), usually caused by antivirus or system scanning, use a separate build directory:

```powershell
$env:CARGO_TARGET_DIR = 'target_tauri'; npm run tauri dev
```

### Port 1420 Already in Use
If you see "Port 1420 is already in use", it means a previous instance is still running. Stop it with `Ctrl+C` or kill the process:
```powershell
Stop-Process -Name "node" -Force
```
