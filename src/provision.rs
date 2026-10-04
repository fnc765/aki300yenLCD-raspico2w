//! Optional one-time local USB build to install PRIVATE settings on the SD card.
//! Normal/CI builds have empty constants. Never upload a provisioned ELF/UF2.
use crate::sdcard::SdVolumeManager;
use embedded_sdmmc::{Mode, VolumeIdx};
include!(concat!(env!("OUT_DIR"), "/provision.rs"));

pub fn install_missing(sd: &SdVolumeManager) -> Result<(), &'static str> {
    if WIFI.is_empty() && MATTER.is_empty() {
        return Ok(());
    }
    let volume = sd.open_volume(VolumeIdx(0)).map_err(|_| "provision: volume")?;
    let root = volume.open_root_dir().map_err(|_| "provision: root")?;
    for (name, bytes) in [("WIFI.TXT", WIFI), ("MATTER.TXT", MATTER)] {
        if bytes.is_empty() {
            continue;
        }
        match root.open_file_in_dir(name, Mode::ReadOnly) {
            Ok(file) => {
                drop(file);
                continue;
            } // Existing settings always win.
            Err(embedded_sdmmc::Error::NotFound) => (),
            Err(_) => return Err("provision: check file"),
        }
        let file = root.open_file_in_dir(name, Mode::ReadWriteCreateOrTruncate).map_err(|_| "provision: create file")?;
        file.write(bytes).map_err(|_| "provision: write file")?;
        file.flush().map_err(|_| "provision: flush file")?;
        file.close().map_err(|_| "provision: close file")?;
    }
    Ok(())
}
