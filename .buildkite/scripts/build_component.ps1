#!/usr/bin/env powershell

#Requires -Version 5

param (
    # The name of the component to be built. Defaults to none
    [string]$Component,
    # The base hab version to run the build with. Defaults to latest
    [string]$BaseHabVersion="latest",
    # The builder channel to pull from. Defaults to stable
    [string]$SourceChannel="stable"
)

# Import shared functions
. "$PSScriptRoot\shared.ps1"

if($Component.Equals("")) {
    Write-Error "--- :error: Component to build not specified, please use the -Component flag" -ErrorAction Stop
}

Write-Host "--- Setting source package channel to $SourceChannel"
$Env:HAB_BLDR_CHANNEL="$SourceChannel"

Write-Host "--- POINT AT ACCEPTANCE!"
$Env:HAB_BLDR_URL="https://bldr.acceptance.habitat.sh"

Write-Host "--- Installing base habitat binary version: $BaseHabVersion"
$baseHabExe = [HabShared]::install_base_habitat_binary($BaseHabVersion, $SourceChannel)
Write-Host "--- Using hab executable at $baseHabExe"

Write-Host "--- Importing Keys"
[HabShared]::import_keys($baseHabExe)

Write-Host "--- Moving build folder to new location"
New-Item -ItemType directory -Path C:\build
Copy-Item -Path C:\workdir\* -Destination C:\build -Recurse

Push-Location "C:\build"
Invoke-Expression "$baseHabExe pkg build components\$Component"
Invoke-Expression "dir results"
Pop-Location
