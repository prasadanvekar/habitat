#!/usr/bin/env powershell

#Requires -Version 5
Set-PSDebug -Trace 1

$Env:HAB_BLDR_CHANNEL="unstable"

Write-Host "--- Setup keys TODO: pull from builder, put in a helper"
Invoke-Expression "hab origin key generate core" -ErrorAction Stop

Write-Host "--- Install latest habitat binary"
Invoke-Expression "choco install habitat --confirm" -ErrorAction Stop

Write-Host "--- Moving build folder to new location"
New-Item -ItemType directory -Path C:\build
Copy-Item -Path C:\workdir\* -Destination C:\build -Recurse

Write-Host "--- Running build"
cd C:\build
Invoke-Expression "hab pkg build components\hab" -ErrorAction Stop

# Invoke-Expression "cargo build --release " -ErrorAction Stop

# Write-Host "--- Uploading artifacts"
# . .\results/last_build.env
# echo $env


exit $LASTEXITCODE
