use log::info;
use std::{fs::{self, File}, io::Write, path};
use oci_distribution::{errors::OciDistributionError, secrets, Client, ParseError, Reference};



const ROOTFS: &str = "application/vnd.ironcore.image.rootfs.v1alpha1.rootfs";
const SQUASHFS: &str = "application/vnd.ironcore.image.squashfs.v1alpha1.squashfs";
const INITRAMFS: &str = "application/vnd.ironcore.image.initramfs.v1alpha1.initramfs";
const VMLINUZ: &str = "application/vnd.ironcore.image.vmlinuz.v1alpha1.vmlinuz";

#[derive(Debug)]
pub enum ImageError {
    InvalidReference(ParseError),
    PullError(OciDistributionError),
    MissingLayer(String),
    IOError(std::io::Error),
}

pub async fn fetch_image(image: String, file_path: path::PathBuf) -> Result<(), ImageError> {
    info!("fetching image: {}", image);
    let reference = Reference::try_from(image.clone()).map_err(ImageError::InvalidReference)?;

    let c = Client::default();

    let media_type = vec![ROOTFS, SQUASHFS, INITRAMFS,VMLINUZ];
    let data = c.pull(&reference, &secrets::RegistryAuth::Anonymous, media_type).await.map_err(ImageError::PullError)?;
    info!("image pulled");


    fs::create_dir_all(file_path.clone()).map_err(ImageError::IOError)?;


    let layers = vec![ROOTFS];
    for layer in layers {
        let kernel = match data.layers.iter().find(|x| x.media_type == layer) {
            Some(x) => x,
            None => return Err(ImageError::MissingLayer(layer.to_string()))
        };
    
        let mut path = file_path.clone();
        path.push(str::replace(layer, "/", "."));

        info!("saving layer: location {:?}, layer: {}", file_path, layer);
        let mut file = File::create(path).map_err(ImageError::IOError)?;
        file.write_all(&kernel.data).map_err(ImageError::IOError)?;
    }
  
    Ok(())
}