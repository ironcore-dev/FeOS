use log::info;
use oci_distribution::manifest::OciManifest;
use oci_distribution::{secrets, Client, Reference};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

const LAYER_COMPRESSED: &str = "application/vnd.oci.image.layer.v1.tar+gzip";
pub const DEFAULT_IMAGE_PATH: &str = "/var/lib/feos/images";

pub async fn fetch_image(image: String) -> Result<String, String> {
    info!("fetching image: {}", image);

    let mut reference = Reference::try_from(image.clone()).map_err(|e| e.to_string())?;

    let c = Client::default();
    let file_path = PathBuf::from(DEFAULT_IMAGE_PATH);

    let manifest = c
        .pull_manifest(&reference, &secrets::RegistryAuth::Anonymous)
        .await
        .map_err(|e| e.to_string())?;
    match manifest.0 {
        OciManifest::Image(_) => {}
        OciManifest::ImageIndex(index) => {
            let layer = index
                .manifests
                .iter()
                .find(|x1| {
                    let platform = match &x1.platform {
                        Some(p) => p,
                        None => return false,
                    };

                    if platform.os == "linux" && platform.architecture == "amd64" {
                        return true;
                    }

                    false
                })
                .unwrap();
            reference = Reference::with_digest(
                reference.registry().to_string(),
                reference.repository().to_string(),
                layer.digest.to_string(),
            );
        }
    };

    if let Some(digest) = reference.digest() {
        let mut path = file_path.clone();
        path.push(digest);
        if Path::new(&path).is_dir() {
            info!("image already present");
            return Ok(digest.to_string());
        }
    }

    let media_type = vec![LAYER_COMPRESSED];
    let data = c
        .pull(&reference, &secrets::RegistryAuth::Anonymous, media_type)
        .await
        .map_err(|e| e.to_string())?;
    info!("image pulled");

    let mut path = file_path.clone();
    let digest = data.digest.unwrap().to_string();
    path.push(digest.clone());
    fs::create_dir_all(path.clone()).map_err(|e| e.to_string())?;

    info!("writing layers to disk");
    for layer in data.layers {
        let mut path = path.clone();
        path.push(layer.sha256_digest());

        let mut file = File::create(path).map_err(|e| e.to_string())?;
        file.write_all(&layer.data).map_err(|e| e.to_string())?;
    }

    Ok(digest)
}
