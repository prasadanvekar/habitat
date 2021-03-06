#!/bin/bash

set -euo pipefail

source .buildkite/scripts/shared.sh

if is_fake_release; then
    bintray_repository=unstable
else
    bintray_repository=stable
fi

echo "--- Preparing to publish artifacts to the ${bintray_repository} Bintray repository"

publish(){
    url=${1}
    if is_fake_release; then
        echo "--- :warning: If this were a real release, we would have hit ${url}"
    else
        curl -u "${BINTRAY_USER}:${BINTRAY_KEY}" -X POST "${url}"
    fi
}

echo "--- :habicat: Publishing all Habitat CLI artifacts in Bintray"

version=$(buildkite-agent meta-data get "version")

########################################################################
# Linux Publish
release=$(buildkite-agent meta-data get "hab-release-linux")
echo "--- :linux: Publishing Linux 'hab' ${version}-${release} on Bintray"
publish "https://api.bintray.com/content/habitat/${bintray_repository}/hab-x86_64-linux/${version}-${release}/publish"

########################################################################
# macOS Publish

release=$(buildkite-agent meta-data get "hab-release-macos")
echo "--- :mac: Publishing macOS 'hab' ${version}-${release} to Bintray"
publish "https://api.bintray.com/content/habitat/${bintray_repository}/hab-x86_64-darwin/${version}-${release}/publish"

########################################################################
# Windows Publish
#
# NOTE: Windows releases aren't yet built in Buildkite, so we have to
# ask Builder what the release actually is... Appveyor puts this here
# for us.
channel=$(buildkite-agent meta-data get "release-channel")
windows_ident=$(latest_from_builder x86_64-windows "${channel}" hab "${version}")
release=$(echo "${windows_ident}" | awk 'BEGIN { FS = "/" } ; { print $4 }')
echo "--- :windows: Publishing Windows 'hab' ${version}-${release}"
publish "https://api.bintray.com/content/habitat/${bintray_repository}/hab-x86_64-windows/${version}-${release}/publish"
