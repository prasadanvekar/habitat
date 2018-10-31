#!/usr/bin/env powershell

#Requires -Version 5
# Set-PSDebug -Trace off

param (
    # The path to the component to be built. If not specified the current directory is used.
    [string]$Path=".",
    # The base hab version to run the build with. Defaults to latest
    [string]$BaseHabVersion="latest",
    # The builder channel to pull from. Defaults to stable
    [string]$SourceChannel="stable"
)

Write-Host "Setting source package channel to $SourceChannel"
$Env:HAB_BLDR_CHANNEL="$SourceChannel"

Write-Host "Installing base habitat binary version: $BaseHabVersion"
$bootstrapDir = "C:\hab-$HabBaseVersion"
$url = "https://api.bintray.com/content/habitat/stable/windows/x86_64/hab-$($BaseHabVersion)-x86_64-windows.zip?bt_package=hab-x86_64-windows"
# Stick the hab 
New-Item -ItemType directory -Path 
# download a hab binary to build hab from source in a studio
Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile hab.zip
Expand-Archive -Path hab.zip -DestinationPath $bootstrapDir -Force
Remove-Item hab.zip -Force
$baseHabExe = (Get-Item "$bootstrapDir\hab.exe").FullName

Write-Host "--- Moving build folder to new location"
New-Item -ItemType directory -Path C:\build
Copy-Item -Path C:\workdir\* -Destination C:\build -Recurse


# Push-Location "$Path"
# Invoke-Expression "hab"
# Pop-Location