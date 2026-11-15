$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $true
if (-not $IsWindows) { throw 'The Rooom Windows gate requires Windows.' }

Push-Location (Join-Path $PSScriptRoot '..')
try {
    $toolchain = Get-Content -LiteralPath rust-toolchain.toml -Raw
    if ($toolchain -notmatch '(?m)^channel\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"') {
        throw 'rust-toolchain.toml must pin a stable release.'
    }
    $rustVersion = $Matches[1]
    $target = 'x86_64-pc-windows-msvc'
    $rust = rustc -Vv
    $rust
    if ("release: $rustVersion" -notin $rust -or "host: $target" -notin $rust) {
        throw "Expected Rust $rustVersion on $target."
    }
    $nodeVersion = (Get-Content -LiteralPath .nvmrc -Raw).Trim()
    $package = Get-Content -LiteralPath package.json -Raw | ConvertFrom-Json
    $pnpmVersion = $package.packageManager -replace '^pnpm@', ''
    if ((node --version) -ne "v$nodeVersion" -or (pnpm --version) -ne $pnpmVersion) {
        throw "Expected Node $nodeVersion and pnpm $pnpmVersion."
    }
    git rev-parse HEAD
    pnpm install --frozen-lockfile
    pnpm test
    pnpm exec tauri build --no-bundle --target $target -- --locked
    cargo test --workspace --locked --target $target
    Get-Item -LiteralPath "target/$target/release/rooom.exe" | Select-Object Name, Length
} finally {
    Pop-Location
}
