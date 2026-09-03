use semver::Version;

pub mod client;
pub mod model;

pub fn library_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap()
}
