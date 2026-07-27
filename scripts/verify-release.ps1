$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$tauriRoot = Join-Path $repoRoot "src-tauri"

Write-Host "== TypeScript and Vite build =="
Push-Location $repoRoot
try {
    npm.cmd run build
}
finally {
    Pop-Location
}

Write-Host "== Rust tests =="
Push-Location $tauriRoot
try {
    cargo check
    cargo test

    Write-Host "== keyring Windows backend feature check =="
    cargo tree -e features -i keyring
}
finally {
    Pop-Location
}

Write-Host "== Release verification passed =="
