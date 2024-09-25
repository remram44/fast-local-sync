use filetime::{FileTime, set_symlink_file_times};
use std::io::ErrorKind;
use std::fs::{copy, create_dir, set_permissions, symlink_metadata};
use std::os::unix::fs::{MetadataExt, chown};
use std::path::Path;
use tracing::debug;

fn copy_metadata(source: &Path, target: &Path) -> std::io::Result<()> {
    // Get metadata of source
    let metadata = symlink_metadata(source)?;

    // Copy attributes
    chown(target, Some(metadata.uid()), Some(metadata.gid()))?;
    set_permissions(target, metadata.permissions())?;
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

    copy(source, target)?;

    copy_metadata(source, target)
}
