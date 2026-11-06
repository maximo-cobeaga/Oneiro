# Builds Oneiro Studio and puts a ready-to-run folder in dist\Oneiro.
#   powershell -ExecutionPolicy Bypass -File tools\package.ps1            # build + dist folder
#   powershell -ExecutionPolicy Bypass -File tools\package.ps1 -Shortcuts # ...and Desktop + Start Menu shortcuts
param([switch]$Shortcuts, [switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
if (-not $SkipBuild) { cargo build --release -p oneiro-ide -p oneiro-cli; if ($LASTEXITCODE -ne 0) { throw 'build failed' } }
$dist = Join-Path $root 'dist\Oneiro'
if (Test-Path $dist) { Remove-Item $dist -Recurse -Force }
New-Item -ItemType Directory -Path $dist | Out-Null
Copy-Item 'target\release\oneiro-ide.exe' (Join-Path $dist 'Oneiro Studio.exe')
Copy-Item 'target\release\oneiro.exe' $dist
Copy-Item 'QUICKSTART.md', 'README.md' $dist
Copy-Item 'docs' (Join-Path $dist 'docs') -Recurse
# Examples, without rendered videos or logs.
$ex = Join-Path $dist 'examples'
Copy-Item 'examples' $ex -Recurse
Get-ChildItem $ex -Recurse -Directory -Filter out | Remove-Item -Recurse -Force
Get-ChildItem $ex -Recurse -Include *.mp4 | Remove-Item -Force
$exe = Join-Path $dist 'Oneiro Studio.exe'
if ($Shortcuts) {
  $ws = New-Object -ComObject WScript.Shell
  $targets = @([Environment]::GetFolderPath('Desktop'), (Join-Path ([Environment]::GetFolderPath('Programs')) ''))
  foreach ($dir in $targets) {
    $lnk = $ws.CreateShortcut((Join-Path $dir 'Oneiro Studio.lnk'))
    $lnk.TargetPath = $exe
    $lnk.WorkingDirectory = $dist
    $lnk.Description = 'Oneiro Studio'
    $lnk.Save()
    Write-Host "shortcut: $dir\Oneiro Studio.lnk"
  }
}
Write-Host "ready: $exe"
