use filetime::{FileTime, set_symlink_file_times};
use std::io::ErrorKind;
use std::fs::{copy, create_dir, read_link, set_permissions, symlink_metadata};
use std::os::unix::fs::{MetadataExt, lchown, symlink};
use std::path::Path;
use tracing::{debug, warn};

fn copy_metadata(source: &Path, target: &Path) -> std::io::Result<()> {
    // Get metadata of source
    let metadata = symlink_metadata(source)?;

    // Copy attributes
    lchown(target, Some(metadata.uid()), Some(metadata.gid()))?;
    if !metadata.is_symlink() {
        set_permissions(target, metadata.permissions())?;
    }
    let mtime = FileTime::from_last_modification_time(&metadata);
    set_symlink_file_times(target, mtime, mtime)?;

    // TODO: Extended ACLs, extended attrs

    Ok(())
}

pub fn copy_directory(source: &Path, target: &Path) -> std::io::Result<()> {
    debug!("copy_directory {:?} {:?}", source, target);

    // Create the directory if it does not exist
    match create_dir(target) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }

    copy_metadata(source, target)
}

pub fn copy_file(source: &Path, target: &Path) -> std::io::Result<()> {
    debug!("copy_file {:?} {:?}", source, target);

    let source_metadata = symlink_metadata(source)?;

    if source_metadata.is_symlink() {
        let link = read_link(source)?;
        symlink(link, target)?;
    } else if source_metadata.is_file() {
        copy(source, target)?;
    } else {
        warn!("Don't know how to copy entry that's not a symlink or a file: {:?}", source);
    }

    copy_metadata(source, target)
}
