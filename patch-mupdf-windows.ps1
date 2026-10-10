# Patch mupdf 0.6.0 for Windows MSVC (max_align_t not defined)
$cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { "$env:USERPROFILE\.cargo" }
$nativeRs = Get-ChildItem -Path "$cargoHome\registry\src" -Recurse -Filter "native.rs" | Where-Object { $_.FullName -like "*mupdf-0.6.0*device*" } | Select-Object -First 1

if ($nativeRs) {
    $content = Get-Content $nativeRs.FullName -Raw
    if ($content -match 'max_align_t') {
        # Replace max_align_t with a fixed 16-byte alignment (same as MSVC max_align_t)
        $patched = $content -replace 'align_of::<max_align_t>\(\)', '16'
        Set-Content $nativeRs.FullName $patched
        Write-Host "Patched: $($nativeRs.FullName)"
    } else {
        Write-Host "Already patched or no max_align_t found."
    }
} else {
    Write-Host "mupdf native.rs not found. Run 'cargo build' first to download crates."
}
