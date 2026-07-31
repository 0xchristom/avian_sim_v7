Write-Host "Building Rust Core..." -ForegroundColor Green
cargo build --release
Write-Host "Building WASM for Viewer..." -ForegroundColor Green
wasm-pack build avian_core --target web --release
Write-Host "Installing Frontend Dependencies..." -ForegroundColor Green
cd viewer
npm install
Write-Host "Starting Viewer..." -ForegroundColor Green
npm run start
