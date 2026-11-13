#requires -Version 7.0
[CmdletBinding()]
param(
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'build/jniLibs'),
    [string]$NdkPath = 'D:\Dev\Android\Sdk\ndk\28.2.13676358',
    [string]$JdkPath = 'D:\Dev\java\jdk-21.0.13+9'
)

$ErrorActionPreference = 'Stop'
$OutputDirectory = $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($OutputDirectory)
$kotlinDirectory = Join-Path $PSScriptRoot 'build/kotlin'
$previousNdk = $env:ANDROID_NDK_HOME
$previousJava = $env:JAVA_HOME

try {
    if (-not (Test-Path -LiteralPath (Join-Path $NdkPath 'source.properties'))) {
        throw "Android NDK not found at $NdkPath"
    }
    if (-not (Test-Path -LiteralPath (Join-Path $JdkPath 'bin/java.exe'))) {
        throw "JDK not found at $JdkPath"
    }
    $env:ANDROID_NDK_HOME = $NdkPath
    $env:JAVA_HOME = $JdkPath
    Push-Location (Split-Path -Parent $PSScriptRoot)
    try {
        & cargo ndk -t arm64-v8a -t x86_64 -P 21 -o $OutputDirectory build `
            --locked --release --package wasm-search-native --lib
        if ($LASTEXITCODE -ne 0) {
            throw "Android library build failed (exit $LASTEXITCODE)"
        }

        # Generate on the host from the metadata embedded in the Android library.
        # uniffi.toml is discovered through Cargo metadata, including the package name.
        $library = Join-Path $OutputDirectory 'arm64-v8a/libwasm_search_native.so'
        & cargo run --locked --release --package wasm-search-native `
            --features bindgen --bin uniffi-bindgen -- generate $library `
            --language kotlin --out-dir $kotlinDirectory --no-format
        if ($LASTEXITCODE -ne 0) {
            throw "Kotlin binding generation failed (exit $LASTEXITCODE)"
        }

        # The same library for Windows, which the Windows client binds from C#.
        & 'C:\Program Files\Microsoft Visual Studio\2022\Community\Common7\Tools\Launch-VsDevShell.ps1' -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
        & cargo build --locked --release --package wasm-search-native --lib --target x86_64-pc-windows-msvc
        if ($LASTEXITCODE -ne 0) { throw "Windows library build failed (exit $LASTEXITCODE)" }
        $windowsDirectory = Join-Path $PSScriptRoot 'build/windows'
        New-Item -ItemType Directory -Force $windowsDirectory | Out-Null
        Copy-Item -LiteralPath (Join-Path (Get-Location) 'target/x86_64-pc-windows-msvc/release/wasm_search_native.dll') -Destination $windowsDirectory -Force
        Write-Host "Windows library: $windowsDirectory"
        Write-Host "Android libraries: $OutputDirectory"
        Write-Host "Kotlin bindings: $kotlinDirectory"
    }
    finally {
        Pop-Location
    }
}
finally {
    $env:ANDROID_NDK_HOME = $previousNdk
    $env:JAVA_HOME = $previousJava
}
