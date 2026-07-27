# Bambu Overlay

A compact, always-on-top desktop overlay for monitoring Bambu Lab 3D printers in real time.

![Windows](https://img.shields.io/badge/Windows-10%2F11-blue)
![License](https://img.shields.io/badge/license-MIT-green)

## Features

- **Live status** — print progress, remaining time, finish time, layer count
- **Temperature monitoring** — nozzle, bed, and chamber with target indicators
- **AMS support** — filament slot details with active slot highlight
- **Alert display** — HMS warnings and print errors
- **Two connection modes** — LAN (direct MQTT) or Bambu Cloud
- **Compact mode** — minimal footprint for streaming/recording setups
- **Click-through** — invisible to mouse clicks when not needed
- **Auto-reconnect** — reconnects automatically if the connection drops

## Installation

Download the latest installer from the [Releases](../../releases) page.

| File | Description |
|------|-------------|
| `Bambu.Overlay_1.0.0_x64-setup.exe` | NSIS installer (recommended) |
| `Bambu.Overlay_1.0.0_x64.msi` | MSI package |

Run the installer and launch **Bambu Overlay** from the Start menu.

## Setup

### LAN mode
1. On your printer: **Settings → Network → LAN Mode** — enable it and note the **LAN Access Code** and **Device ID**
2. In the overlay: open **Settings → Printer → LAN**, enter the printer IP, Device ID, and LAN Access Code

### Cloud mode
1. In the overlay: open **Settings → Printer → Cloud**
2. Log in with your Bambu Lab account — a verification code will be sent to your email or phone
3. Select your printer from the device list

## Building from source

Requirements: [Rust](https://rustup.rs/) · [Node.js 20+](https://nodejs.org/) · [Tauri CLI v2](https://tauri.app/)

```sh
npm install
npm run tauri:build
```

The installer will be in `src-tauri/target/release/bundle/`.

### Development checks

```sh
npm run dev:mock
npm run verify:release
```

- `dev:mock` starts a dedicated mock UI mode with print, alert, and AMS sample data. It does not connect to a printer.
- `tauri:dev:mock` starts the same mock UI inside the Tauri shell for window-size smoke checks.
- `verify:release` runs the frontend build, Rust tests, and the Windows keyring feature check before packaging.
- In the app, open **Settings → Display → AMS payload** and press **Check** to inspect the latest received printer payload used for AMS parsing.

---

# Bambu Overlay（日本語）

Bambu Lab 3Dプリンターをリアルタイムで監視するコンパクトなデスクトップオーバーレイです。

## 主な機能

- **印刷進捗のリアルタイム表示** — 進捗率・残り時間・完了予定時刻・レイヤー数
- **温度監視** — ノズル・ベッド・チャンバー（目標値との比較付き）
- **AMSサポート** — フィラメントスロットの詳細とアクティブスロットのハイライト
- **アラート表示** — HMS警告・印刷エラー
- **2つの接続方式** — LAN（直接MQTT）またはBambu Cloud
- **コンパクトモード** — 配信/録画向けの小さいフットプリント
- **クリック透過** — 必要ないときはマウス操作を素通りさせる
- **自動再接続** — 接続が切れても自動で再試みる

## インストール

[Releases](../../releases) ページから最新のインストーラーをダウンロードしてください。

| ファイル | 説明 |
|---------|------|
| `Bambu.Overlay_1.0.0_x64-setup.exe` | NSlSインストーラー（推奨） |
| `Bambu.Overlay_1.0.0_x64.msi` | MSIパッケージ |

インストール後、スタートメニューから **Bambu Overlay** を起動してください。

## 設定

### LANモード
1. プリンター本体：**設定 → ネットワーク → LAN Mode** を有効にし、**LANアクセスコード** と **Device ID** を確認する
2. オーバーレイ：**設定 → プリンター → LAN** でプリンターIP・Device ID・LANアクセスコードを入力する

### Cloudモード
1. オーバーレイ：**設定 → プリンター → Cloud** を開く
2. Bambu Labアカウントでログイン（メールまたはSMSに認証コードが届く）
3. デバイス一覧からプリンターを選択する

## ソースからビルド

必要なもの：[Rust](https://rustup.rs/) · [Node.js 20+](https://nodejs.org/) · [Tauri CLI v2](https://tauri.app/)

```sh
npm install
npm run tauri:build
```

インストーラーは `src-tauri/target/release/bundle/` に生成されます。

### 開発・リリース前確認

```sh
npm run dev:mock
npm run verify:release
```

- `dev:mock` は印刷中・アラート・AMSサンプルを使う専用モックUIです。実機には接続しません。
- `tauri:dev:mock` は同じモックUIをTauriウィンドウで起動し、展開/格納やコンパクト表示の見切れ確認に使います。
- `verify:release` はフロントエンドビルド、Rustテスト、Windows keyring feature確認をまとめて実行します。
- 実機AMSの確認は **設定 → 表示 → AMS payload** の **確認** から直近payloadを見ます。
