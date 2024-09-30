use filetime::{FileTime, set_symlink_file_times};
use std::fs::{copy, create_dir, read_link, remove_file, set_permissions, symlink_metadata};
use std::io::ErrorKind;
use std::os::unix::fs::{MetadataExt, lchown, symlink};
use std::path::Path;
use tracing::debug;

// Metadata copied unconditionally
pub fn copy_extended_metadata(source: &Path, target: &Path, is_dir: bool) -> std::io::Result<()> {
    #[cfg(feature = "acl")]
    {
        use exacl::{AclOption, getfacl, setfacl};

        let acl = getfacl(source, Some(AclOption::ACCESS_ACL))?;
        setfacl(&[target], &acl, Some(AclOption::ACCESS_ACL))?;

        if is_dir {
            let default_acl = getfacl(source, Some(AclOption::DEFAULT_ACL))?;
            setfacl(&[target], &default_acl, Some(AclOption::DEFAULT_ACL))?;
        }
    }

    #[cfg(feature = "attr")]
    {
        use std::collections::HashSet;
        use std::os::unix::ffi::OsStrExt;
        use xattr::{get, list, remove, set};

        let mut seen_attrs = HashSet::new();

        for name in list(source)? {
            let name_b = &name.as_bytes();
            if name_b.len() >= 7 && &name_b[0..7] == b"system." {
                continue;
            }

            if let Some(value) = get(source, &name)? {
                set(target, &name, &value)?;
                seen_attrs.insert(name);
            }
        }

        for name in list(target)? {
            let name_b = &name.as_bytes();
            if name_b.len() >= 7 && &name_b[0..7] == b"system." {
                continue;
            }

            if !seen_attrs.contains(&name) {
                remove(target, name)?;
            }
        }
    }

    Ok(())
}

// Metadata copied when the file is copied
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

    copy_extended_metadata(source, target, metadata.is_dir())?;

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
        debug!("copy_file symlink {:?} -> {:?}", link, target);
        match remove_file(target) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        symlink(link, target)?;
    } else if source_metadata.is_file() {
        debug!("copy_file regular file {:?} -> {:?}", source, target);
        copy(source, target)?;
    } else {
        return Err(std::io::Error::new(
            ErrorKind::Other,
            format!("Don't know how to copy entry that's not a symlink or a file: {:?}", source),
        ));
    };

    copy_metadata(source, target)
}
