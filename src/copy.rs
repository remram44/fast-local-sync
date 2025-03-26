use filetime::{FileTime, set_symlink_file_times};
use std::fs::{Metadata, copy, create_dir, read_link, remove_file, set_permissions, symlink_metadata};
use std::io::ErrorKind;
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
    #[cfg(target_family = "unix")]
    {
        use std::os::unix::fs::{MetadataExt, lchown};
        lchown(target, Some(metadata.uid()), Some(metadata.gid()))?;
    }
    if !metadata.is_symlink() {
        set_permissions(target, metadata.permissions())?;
    }
    let mtime = FileTime::from_last_modification_time(&metadata);
    set_symlink_file_times(target, mtime, mtime)?;

    if !metadata.is_symlink() {
        copy_extended_metadata(source, target, metadata.is_dir())?;
    }

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

fn copy_data(source: &Path, source_metadata: &Metadata, target: &Path) -> std::io::Result<u64> {
    if source_metadata.is_symlink() {
        let link = read_link(source)?;
        debug!("copy_file symlink {:?} -> {:?}", link, target);
        match remove_file(target) {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        #[cfg(target_family = "unix")]
        {
            std::os::unix::fs::symlink(link, target)?;
        }
        #[cfg(not(target_family = "unix"))]
        {
            return Err(std::io::Error::new(
                ErrorKind::Other,
                "Creating symlinks is not supported on this platform",
            ));
        }
        return Ok(0);
    }

    if source_metadata.is_file() {
        debug!("copy_file regular file {:?} -> {:?}", source, target);
        return copy(source, target);
    }

    #[cfg(feature = "unixdev")]
    {
        use nix::sys::stat::{Mode, SFlag, mknod};
        use std::os::unix::fs::{FileTypeExt, PermissionsExt};

        if source_metadata.file_type().is_socket() {
            mknod(
                target,
                SFlag::S_IFSOCK,
                Mode::from_bits(
                    source_metadata.permissions().mode(),
                ).unwrap(),
                0,
            )?;
            return Ok(0);
        }
    }

    #[cfg(feature = "unixdev")]
    {
        use nix::sys::stat::Mode;
        use nix::unistd::mkfifo;
        use std::os::unix::fs::{FileTypeExt, PermissionsExt};

        if source_metadata.file_type().is_fifo() {
            mkfifo(
                target,
                Mode::from_bits(
                    source_metadata.permissions().mode(),
                ).unwrap(),
            )?;
            return Ok(0);
        }
    }

    #[cfg(feature = "unixdev")]
    {
        use nix::sys::stat::{Mode, SFlag, mknod, lstat};
        use std::os::unix::fs::PermissionsExt;

        let file_stat = lstat(source)?;

        // Block device
        if file_stat.st_mode & SFlag::S_IFBLK.bits() != 0 {
            mknod(
                target,
                SFlag::S_IFBLK,
                Mode::from_bits(
                    source_metadata.permissions().mode(),
                ).unwrap(),
                file_stat.st_dev,
            )?;
            return Ok(0);
        }

        // Character device
        if file_stat.st_mode & SFlag::S_IFCHR.bits() != 0 {
            mknod(
                target,
                SFlag::S_IFCHR,
                Mode::from_bits(
                    source_metadata.permissions().mode(),
                ).unwrap(),
                file_stat.st_dev,
            )?;
            return Ok(0);
        }
    }

    Err(std::io::Error::new(
        ErrorKind::Other,
        format!("Don't know how to copy entry, type unsupported: {:?}", source),
    ))
}

pub fn copy_file(source: &Path, target: &Path) -> std::io::Result<u64> {
    debug!("copy_file {:?} {:?}", source, target);

    let source_metadata = symlink_metadata(source)?;

    let size = copy_data(source, &source_metadata, target)?;

    copy_metadata(source, target)?;

    Ok(size)
}
