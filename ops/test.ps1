[CmdletBinding()]
param(
    [ValidateSet('quick', 'full')]
    [string]$Mode = 'quick',

    [string]$CorpusPath,
    [string]$BundlePath,
    [string]$BaselineBundlePath,
    [string]$ExamSourcePath,
    [string]$ExamMaterializedPath,
    [string]$ExamCachePath,
    [string]$ExamSnapshotPath,
    [string]$ExamHistoryPath,
    [string]$RoomOccupancyPath,
    [string]$TeachingSourcePath,
    [string]$TeachingSchedulePath,
    [string]$TeachingRoomOccupancyPath,
    [string]$SpaceSnapshotPath,
    [string]$WebStagePath,
    [string]$WebDistPath,
    [string]$CargoTargetPath
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
if (-not $CargoTargetPath) {
    $CargoTargetPath = if ($env:RUNNER_TEMP) {
        Join-Path $env:RUNNER_TEMP 'njupt-search-cargo-target'
    }
    elseif ($IsWindows) {
        'D:\Cache\njupt-search\cargo-target'
    }
    else {
        Join-Path ([System.IO.Path]::GetTempPath()) 'njupt-search-cargo-target'
    }
}
$cargoTarget = [System.IO.Path]::GetFullPath($CargoTargetPath)
[System.IO.Directory]::CreateDirectory($cargoTarget) | Out-Null
$previousCargoTarget = $env:CARGO_TARGET_DIR
$env:CARGO_TARGET_DIR = $cargoTarget

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [scriptblock]$Command,

        [Parameter(Mandatory = $true)]
        [string]$Failure
    )
    & $Command
    if ($LASTEXITCODE -ne 0) { throw $Failure }
}

Push-Location $repository
try {
    Invoke-Checked {
        node --test .github/scripts/parse-edgeone-deploy-result.test.mjs
    } 'Deployment helper tests failed'
    Invoke-Checked {
        node --test middleware.test.mjs
    } 'Query indexing middleware tests failed'
    if ($IsLinux) {
        Invoke-Checked {
            bash .github/scripts/github-retry.test.sh
        } 'GitHub retry helper tests failed'
    }
    Invoke-Checked { cargo test --manifest-path search/Cargo.toml --workspace } 'Rust tests failed'
    Invoke-Checked { npm run build:wasm:web } 'WASM build failed'
    Invoke-Checked { npm run lint } 'Lint failed'
    Invoke-Checked { npm run typecheck:prepared } 'TypeScript typecheck failed'
    Invoke-Checked { npm run test:prepared } 'TypeScript tests failed'
    Invoke-Checked { uv sync --extra test } 'Python test environment sync failed'
    Invoke-Checked {
        uv run pytest academics/exam academics/room/occupancy academics/timetable academics/space -q
    } 'Academics tests failed'

    if ($Mode -eq 'full') {
        $required = @{
            CorpusPath = $CorpusPath
            BundlePath = $BundlePath
            ExamSourcePath = $ExamSourcePath
            ExamMaterializedPath = $ExamMaterializedPath
            ExamCachePath = $ExamCachePath
            ExamSnapshotPath = $ExamSnapshotPath
            ExamHistoryPath = $ExamHistoryPath
            RoomOccupancyPath = $RoomOccupancyPath
            TeachingSourcePath = $TeachingSourcePath
            TeachingSchedulePath = $TeachingSchedulePath
            TeachingRoomOccupancyPath = $TeachingRoomOccupancyPath
            SpaceSnapshotPath = $SpaceSnapshotPath
            WebStagePath = $WebStagePath
            WebDistPath = $WebDistPath
        }
        foreach ($entry in $required.GetEnumerator()) {
            if (-not $entry.Value) { throw "-$($entry.Key) is required for a full test" }
        }

        & (Join-Path $PSScriptRoot 'build-search-bundle.ps1') `
            -CorpusPath $CorpusPath `
            -BundlePath $BundlePath `
            -CargoTargetPath $cargoTarget

        $qualityArguments = @(
            'benchmarks/search/quality.mjs',
            '--bundle', $BundlePath
        )
        if ($BaselineBundlePath) {
            $qualityArguments += @('--baseline', $BaselineBundlePath)
        }
        Invoke-Checked { node @qualityArguments } 'Search quality benchmark failed'

        & (Join-Path $PSScriptRoot 'build-academics.ps1') `
            -SourcePath $ExamSourcePath `
            -MaterializedPath $ExamMaterializedPath `
            -CachePath $ExamCachePath `
            -ExamOutputPath $ExamSnapshotPath `
            -HistoryOutputPath $ExamHistoryPath `
            -RoomOutputPath $RoomOccupancyPath `
            -TeachingSourcePath $TeachingSourcePath `
            -TeachingOutputPath $TeachingSchedulePath `
            -TeachingRoomOutputPath $TeachingRoomOccupancyPath `
            -SpaceSnapshotPath $SpaceSnapshotPath
        Invoke-Checked {
            npm run academics:validate -- `
                --exam $ExamSnapshotPath `
                --history $ExamHistoryPath `
                --room $RoomOccupancyPath `
                --timetable $TeachingSchedulePath `
                --classrooms $TeachingRoomOccupancyPath `
                --space $SpaceSnapshotPath
        } 'Academics producer/consumer validation failed'

        & (Join-Path $PSScriptRoot 'assemble-web.ps1') `
            -SearchBundlePath $BundlePath `
            -ExamSnapshotPath $ExamSnapshotPath `
            -ExamHistoryPath $ExamHistoryPath `
            -RoomOccupancyPath $RoomOccupancyPath `
            -TeachingSchedulePath $TeachingSchedulePath `
            -TeachingRoomOccupancyPath $TeachingRoomOccupancyPath `
            -SpaceSnapshotPath $SpaceSnapshotPath `
            -StagePath $WebStagePath `
            -DistPath $WebDistPath
    }
}
finally {
    Pop-Location
    $env:CARGO_TARGET_DIR = $previousCargoTarget
}
