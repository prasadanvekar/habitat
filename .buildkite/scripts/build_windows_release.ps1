#!/usr/bin/env powershell

#Requires -Version 5
Set-PSDebug -Trace 1

$Env:HAB_BLDR_CHANNEL="unstable"

Write-Host "--- Install latest habitat binary"
Invoke-Expression "choco install habitat --confirm" -ErrorAction Stop

Write-Host "--- Moving build folder to new location"
New-Item -ItemType directory -Path C:\build
Copy-Item -Path C:\workdir\* -Destination C:\build -Recurse

cd C:\build

Write-Host "--- Setup keys TODO: pull from builder, put in a helper"
Invoke-Expression "hab origin key generate core" -ErrorAction Stop

Write-Host "--- Running build"
Invoke-Expression "hab pkg build components\hab -k core" -ErrorAction Stop

Write-Host "--- Uploading artifacts"

exit $LASTEXITCODE
