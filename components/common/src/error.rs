// Copyright (c) 2016-2017 Chef Software Inc. and/or applicable contributors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::env;
use std::error;
use std::fmt;
use std::io;
use std::result;
use std::str;
use std::string;
use toml;

use api_client;
use handlebars;
use hcore;
use hcore::package::{PackageIdent, PackageInstall};
use serde_json;

pub type Result<T> = result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    APIClient(api_client::Error),
    ArtifactIdentMismatch((String, String, String)),
    BadEnvConfig(String),
    BadPackage(PackageInstall, hcore::error::Error),
    CantUploadGossipToml,
    ChannelNotFound,
    CryptoKeyError(String),
    GossipFileRelativePath(String),
    DownloadFailed(String),
    EditStatus,
    EnvJoinPathsError(env::JoinPathsError),
    FileNameError,
    FileNotFound(String),
    HabitatCore(hcore::Error),
    InvalidInstallHookMode(String),
    /// Occurs when making lower level IO calls.
    IO(io::Error),
    OfflineArtifactNotFound(PackageIdent),
    OfflineOriginKeyNotFound(String),
    OfflinePackageNotFound(PackageIdent),
    RootRequired,
    StrFromUtf8Error(str::Utf8Error),
    StringFromUtf8Error(string::FromUtf8Error),
    TomlSerializeError(toml::ser::Error),
    WireDecode(String),
    EditorEnv(env::VarError),
    PackageNotFound(String),
    Permissions(String),
    RenderContextSerialization(serde_json::Error),
    TemplateFileError(handlebars::TemplateFileError),
    TemplateRenderError(handlebars::RenderError),
    TomlMergeError(String),
    TomlParser(toml::de::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let msg = match *self {
            Error::APIClient(ref err) => format!("{}", err),
            Error::ArtifactIdentMismatch((ref a, ref ai, ref i)) => format!(
                "Artifact ident {} for `{}' does not match expected ident {}",
                ai, a, i
            ),
            Error::BadEnvConfig(ref varname) => {
                format!("Unable to find valid TOML or JSON in {} ENVVAR", varname)
            }
            Error::BadPackage(ref pkg, ref err) => format!("Bad package, {}, {}", pkg, err),
            Error::CantUploadGossipToml => {
                format!("Can't upload gossip.toml, it's a reserved file name")
            }
            Error::ChannelNotFound => format!("Channel not found"),
            Error::CryptoKeyError(ref s) => format!("Missing or invalid key: {}", s),
            Error::GossipFileRelativePath(ref s) => format!(
                "Path for gossip file cannot have relative components (eg: ..): {}",
                s
            ),
            Error::DownloadFailed(ref msg) => format!("{}", msg),
            Error::EditStatus => format!("Failed edit text command"),
            Error::EnvJoinPathsError(ref err) => format!("{}", err),
            Error::FileNameError => format!("Failed to extract a filename"),
            Error::FileNotFound(ref e) => format!("File not found at: {}", e),
            Error::HabitatCore(ref e) => format!("{}", e),
            Error::InvalidInstallHookMode(ref e) => format!("Invalid InstallHookMode conversion from {}", e),
            Error::IO(ref err) => format!("{}", err),
            Error::OfflineArtifactNotFound(ref ident) => {
                format!("Cached artifact not found in offline mode: {}", ident)
            }
            Error::OfflineOriginKeyNotFound(ref name_with_rev) => format!(
                "Cached origin key not found in offline mode: {}",
                name_with_rev
            ),
            Error::OfflinePackageNotFound(ref ident) => format!(
                "No installed package or cached artifact could be found \
                 locally in offline mode: {}",
                ident
            ),
            Error::Permissions(ref err) => format!("{}", err),
            Error::RootRequired => {
                "Root or administrator permissions required to complete operation".to_string()
            }
            Error::StrFromUtf8Error(ref e) => format!("{}", e),
            Error::StringFromUtf8Error(ref e) => format!("{}", e),
            Error::TomlSerializeError(ref e) => format!("Can't serialize TOML: {}", e),
            Error::WireDecode(ref m) => format!("Failed to decode wire message: {}", m),
            Error::EditorEnv(ref e) => format!("Missing EDITOR environment variable: {}", e),
            Error::PackageNotFound(ref e) => format!("Package not found. {}", e),
            Error::RenderContextSerialization(ref e) => {
                format!("Unable to serialize rendering context, {}", e)
            }
            Error::TemplateFileError(ref err) => format!("{:?}", err),
            Error::TemplateRenderError(ref err) => format!("{}", err),
            Error::TomlMergeError(ref e) => format!("Failed to merge TOML: {}", e),
            Error::TomlParser(ref err) => format!("Failed to parse TOML: {}", err),
        };
        write!(f, "{}", msg)
    }
}

impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::APIClient(ref err) => err.description(),
            Error::ArtifactIdentMismatch((_, _, _)) => {
                "Artifact ident does not match expected ident"
            }
            Error::BadEnvConfig(_) => "Unknown syntax in Env Configuration",
            Error::BadPackage(_, _) => "Package was malformed or contained malformed contents",
            Error::CantUploadGossipToml => "Can't upload gossip.toml, it's a reserved filename",
            Error::ChannelNotFound => "Channel not found",
            Error::CryptoKeyError(_) => "Missing or invalid key",
            Error::DownloadFailed(_) => "Failed to download from remote",
            Error::GossipFileRelativePath(_) => {
                "Path for gossip file cannot have relative components (eg: ..)"
            }
            Error::EditStatus => "Failed edit text command",
            Error::EnvJoinPathsError(ref err) => err.description(),
            Error::FileNameError => "Failed to extract a filename from a path",
            Error::FileNotFound(_) => "File not found",
            Error::HabitatCore(ref err) => err.description(),
            Error::InvalidInstallHookMode(_) => "Invalid InstallHookMode",
            Error::IO(ref err) => err.description(),
            Error::OfflineArtifactNotFound(_) => "Cached artifact not found in offline mode",
            Error::OfflineOriginKeyNotFound(_) => "Cached origin key not found in offline mode",
            Error::OfflinePackageNotFound(_) => {
                "No installed package or cached artifact could be found locally in offline mode"
            }
            Error::Permissions(_) => "File system permissions error",
            Error::RootRequired => {
                "Root or administrator permissions required to complete operation"
            }
            Error::StrFromUtf8Error(_) => "Failed to convert a string as UTF-8",
            Error::StringFromUtf8Error(_) => "Failed to convert a string as UTF-8",
            Error::TemplateFileError(ref err) => err.description(),
            Error::TomlSerializeError(_) => "Can't serialize TOML",
            Error::WireDecode(_) => "Failed to decode wire message",
            Error::EditorEnv(_) => "Missing EDITOR environment variable",
            Error::PackageNotFound(_) => "Package not found",
            Error::RenderContextSerialization(_) => "Unable to serialize rendering context",
            Error::TemplateRenderError(ref err) => err.description(),
            Error::TomlMergeError(_) => "Failed to merge TOML!",
            Error::TomlParser(_) => "Failed to parse TOML!",
        }
    }
}

impl From<api_client::Error> for Error {
    fn from(err: api_client::Error) -> Self {
        Error::APIClient(err)
    }
}

impl From<hcore::Error> for Error {
    fn from(err: hcore::Error) -> Self {
        Error::HabitatCore(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Error::IO(err)
    }
}

impl From<str::Utf8Error> for Error {
    fn from(err: str::Utf8Error) -> Self {
        Error::StrFromUtf8Error(err)
    }
}

impl From<string::FromUtf8Error> for Error {
    fn from(err: string::FromUtf8Error) -> Self {
        Error::StringFromUtf8Error(err)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(err: toml::ser::Error) -> Self {
        Error::TomlSerializeError(err)
    }
}

impl From<env::JoinPathsError> for Error {
    fn from(err: env::JoinPathsError) -> Self {
        Error::EnvJoinPathsError(err)
    }
}

impl From<handlebars::TemplateFileError> for Error {
    fn from(err: handlebars::TemplateFileError) -> Self {
        Error::TemplateFileError(err)
    }
}

impl From<toml::de::Error> for Error {
    fn from(err: toml::de::Error) -> Self {
        Error::TomlParser(err)
    }
}

impl From<handlebars::RenderError> for Error {
    fn from(err: handlebars::RenderError) -> Self {
        Error::TemplateRenderError(err)
    }
}
