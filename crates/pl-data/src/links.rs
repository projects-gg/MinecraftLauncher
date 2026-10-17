//! Links inside the data folder: symbolic links, and junctions on Windows (a player may keep
//! `saves/` or `assets/` on another disk through one). A move makes each of them again in the
//! copy, pointing at the same place, and never copies through it: the target may be a big folder
//! on another disk. Deleting a folder never follows a link either (`std::fs::remove_dir_all`
//! removes the link itself), so the old folder's links go without their targets.

use std::io;
use std::path::{Component, Path, PathBuf};

/// Whether the link at `path` stands for a folder (Windows records this on the link itself; on
/// Unix it is whatever the target is now, and a dangling link counts as a file link).
pub(crate) fn is_folder_link(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink_dir())
    }
    #[cfg(not(windows))]
    {
        std::fs::metadata(path).is_ok_and(|meta| meta.is_dir())
    }
}

/// `path` with `.` and `..` resolved by name (nothing is read from the disk; the target of a
/// link may not exist) and a Windows `\\?\C:\` prefix written as `C:\`.
fn normalize(path: &Path) -> PathBuf {
    let path = plain_disk_path(path);
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(windows)]
fn plain_disk_path(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest)
            if rest.as_bytes().get(1) == Some(&b':')
                && rest.as_bytes().first().is_some_and(u8::is_ascii_alphabetic) =>
        {
            PathBuf::from(rest)
        }
        _ => path.to_path_buf(),
    }
}

#[cfg(not(windows))]
fn plain_disk_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

/// Where `target`, as stored in the link at `link`, leads: made absolute against the folder
/// holding the link.
pub(crate) fn absolute_target(link: &Path, target: &Path) -> PathBuf {
    if target.is_absolute() {
        normalize(target)
    } else {
        normalize(&link.parent().unwrap_or_else(|| Path::new("")).join(target))
    }
}

/// Path components compared the way the file system usually does (case folded on Windows and
/// macOS).
fn folded(path: &Path) -> Vec<String> {
    path.components()
        .map(|component| {
            let text = component.as_os_str().to_string_lossy();
            if cfg!(any(windows, target_os = "macos")) {
                text.to_lowercase()
            } else {
                text.into_owned()
            }
        })
        .collect()
}

/// `path` relative to `root` when it is `root` or inside it (by name; both absolute).
fn inside(path: &Path, root: &Path) -> Option<PathBuf> {
    let (path, root) = (normalize(path), normalize(root));
    let (outer, inner) = (folded(&root), folded(&path));
    (inner.len() >= outer.len() && inner[..outer.len()] == outer[..])
        .then(|| path.components().skip(outer.len()).collect())
}

/// Whether the relative `target` of a link at `rel` (relative to the data folder) climbs above
/// the data folder on its way (`../../../Other/x`): such a target only works from where the data
/// folder is now.
fn climbs_out(rel: &Path, target: &Path) -> bool {
    let mut depth = rel.parent().map_or(0, |parent| parent.components().count());
    for component in target.components() {
        match component {
            Component::ParentDir if depth == 0 => return true,
            Component::ParentDir => depth -= 1,
            Component::Normal(_) => depth += 1,
            _ => {}
        }
    }
    false
}

/// What the copy of the link `rel` points at when the data folder `src_root` is copied to
/// `dst_root`, given its stored `target`: the same place as the original, except that a link into
/// the data folder points at the same place in the copy (the original goes away after the move).
/// A relative target that stays inside the data folder keeps its form (it resolves the same way
/// in the copy); every other target becomes absolute.
pub(crate) fn target_in_copy(
    src_root: &Path,
    dst_root: &Path,
    rel: &Path,
    target: &Path,
) -> PathBuf {
    let absolute = absolute_target(&src_root.join(rel), target);
    match inside(&absolute, src_root) {
        Some(_) if target.is_relative() && !climbs_out(rel, target) => target.to_path_buf(),
        Some(inner) => dst_root.join(inner),
        None => absolute,
    }
}

/// Whether two stored targets of the link at `link` lead to the same place.
pub(crate) fn same_target(link: &Path, a: &Path, b: &Path) -> bool {
    folded(&absolute_target(link, a)) == folded(&absolute_target(link, b))
}

/// Makes `link` (which must not exist) point at `target`. On Windows a folder link becomes a
/// junction, which needs no extra rights (a symbolic link needs Developer Mode or an
/// administrator); only a target on a network share, which junctions cannot reach, is tried as a
/// symbolic link. A file link is always a symbolic link.
pub(crate) fn create(link: &Path, target: &Path, folder: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        let _ = folder;
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        if !folder {
            return std::os::windows::fs::symlink_file(target, link);
        }
        let absolute = absolute_target(link, target);
        if matches!(
            absolute.components().next(),
            Some(Component::Prefix(prefix)) if matches!(
                prefix.kind(),
                std::path::Prefix::UNC(..) | std::path::Prefix::VerbatimUNC(..)
            )
        ) {
            std::os::windows::fs::symlink_dir(target, link)
        } else {
            junction::create(link, &absolute)
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (link, target, folder);
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

/// The OS refused to make a symbolic link for lack of rights (Windows without Developer Mode).
pub(crate) fn needs_privilege(error: &io::Error) -> bool {
    // ERROR_PRIVILEGE_NOT_HELD
    cfg!(windows) && error.raw_os_error() == Some(1314)
}

/// Removes the link (or the plain file a file link was copied as) at `path`, never its target.
/// A real folder in its place is left alone.
pub(crate) fn remove(path: &Path) -> io::Result<()> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        // A folder link (junction or symbolic link) is removed like an empty folder; that never
        // touches what it points at.
        if meta.file_type().is_symlink_dir() {
            return std::fs::remove_dir(path);
        }
    }
    if meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "a folder is where a link belongs",
        ));
    }
    std::fs::remove_file(path)
}

#[cfg(windows)]
mod junction {
    //! Junctions (NTFS mount points to a folder): a reparse point set on an empty folder with
    //! `FSCTL_SET_REPARSE_POINT`, as `mklink /J` does. No extra rights are needed.

    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::IO::DeviceIoControl;

    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
    const IO_REPARSE_TAG_MOUNT_POINT: u32 = 0xA000_0003;
    /// `MAXIMUM_REPARSE_DATA_BUFFER_SIZE`.
    const MAX_BUFFER: usize = 16 * 1024;

    /// Makes `link` (which must not exist) a junction to the absolute local folder `target`.
    pub(super) fn create(link: &Path, target: &Path) -> io::Result<()> {
        let buffer = mount_point(target)?;
        std::fs::create_dir(link)?;
        let set = set_reparse_point(link, &buffer);
        if set.is_err() {
            let _ = std::fs::remove_dir(link);
        }
        set
    }

    fn set_reparse_point(link: &Path, buffer: &[u8]) -> io::Result<()> {
        let folder = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(link)?;
        let size = u32::try_from(buffer.len()).map_err(|_| io::ErrorKind::InvalidInput)?;
        let mut returned = 0u32;
        // SAFETY: the handle stays open for the whole call, `buffer` is a complete
        // REPARSE_DATA_BUFFER of `size` bytes that outlives it, and no output buffer is passed.
        let result = unsafe {
            DeviceIoControl(
                HANDLE(folder.as_raw_handle()),
                FSCTL_SET_REPARSE_POINT,
                Some(buffer.as_ptr().cast()),
                size,
                None,
                0,
                Some(&raw mut returned),
                None,
            )
        };
        result.map_err(|error| {
            // HRESULT_FROM_WIN32 back to the Win32 code, so `io::ErrorKind` reads right.
            let code = error.code().0;
            if (code.cast_unsigned() & 0xFFFF_0000) == 0x8007_0000 {
                io::Error::from_raw_os_error(code & 0xFFFF)
            } else {
                io::Error::other(error)
            }
        })
    }

    /// A mount point `REPARSE_DATA_BUFFER`: `\??\<target>` as the substitute name, the target
    /// itself as the print name, each followed by a NUL.
    fn mount_point(target: &Path) -> io::Result<Vec<u8>> {
        let text: Vec<u16> = target.as_os_str().encode_wide().collect();
        let verbatim: Vec<u16> = r"\\?\".encode_utf16().collect();
        let print: &[u16] = text.strip_prefix(verbatim.as_slice()).unwrap_or(&text);
        let mut substitute: Vec<u16> = r"\??\".encode_utf16().collect();
        substitute.extend_from_slice(print);
        let too_long = || io::Error::new(io::ErrorKind::InvalidInput, "link target too long");
        let substitute_bytes = u16::try_from(substitute.len() * 2).map_err(|_| too_long())?;
        let print_bytes = u16::try_from(print.len() * 2).map_err(|_| too_long())?;
        // Four u16 offsets/lengths, then the two names with their NULs.
        let data = 8 + usize::from(substitute_bytes) + 2 + usize::from(print_bytes) + 2;
        if 8 + data > MAX_BUFFER {
            return Err(too_long());
        }
        let data = u16::try_from(data).map_err(|_| too_long())?;
        let mut buffer = Vec::with_capacity(8 + usize::from(data));
        buffer.extend_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
        buffer.extend_from_slice(&data.to_le_bytes());
        buffer.extend_from_slice(&0u16.to_le_bytes());
        buffer.extend_from_slice(&0u16.to_le_bytes());
        buffer.extend_from_slice(&substitute_bytes.to_le_bytes());
        buffer.extend_from_slice(&(substitute_bytes + 2).to_le_bytes());
        buffer.extend_from_slice(&print_bytes.to_le_bytes());
        for unit in substitute.iter().chain(&[0]).chain(print).chain(&[0]) {
            buffer.extend_from_slice(&unit.to_le_bytes());
        }
        Ok(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs(path: &str) -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(format!(r"C:{}", path.replace('/', "\\")))
        } else {
            PathBuf::from(path)
        }
    }

    #[test]
    fn targets_follow_the_copy_only_when_they_lead_inside() {
        let (src, dst) = (abs("/data/ProjectsLauncher"), abs("/new/ProjectsLauncher"));
        let rel = Path::new("instances/a/minecraft/saves");
        // Outside the data folder: the very same place, absolute.
        assert_eq!(
            target_in_copy(&src, &dst, rel, &abs("/worlds")),
            abs("/worlds")
        );
        assert_eq!(
            target_in_copy(&src, &dst, rel, Path::new("../../../../worlds")),
            abs("/data/worlds")
        );
        // Inside: the same place in the copy.
        assert_eq!(
            target_in_copy(&src, &dst, rel, &abs("/data/ProjectsLauncher/shared/saves")),
            abs("/new/ProjectsLauncher/shared/saves")
        );
        // A relative target that stays inside keeps its form.
        assert_eq!(
            target_in_copy(&src, &dst, rel, Path::new("../../b/minecraft/saves")),
            PathBuf::from("../../b/minecraft/saves")
        );
        // One that climbs above the data folder and comes back by name is rewritten.
        assert_eq!(
            target_in_copy(
                &src,
                &dst,
                rel,
                Path::new("../../../../ProjectsLauncher/shared")
            ),
            abs("/new/ProjectsLauncher/shared")
        );
        // The data folder itself is inside too.
        assert_eq!(target_in_copy(&src, &dst, rel, &src), dst.clone());
        if cfg!(windows) {
            // Letter case and the `\\?\` prefix do not hide the data folder.
            assert_eq!(
                target_in_copy(
                    &src,
                    &dst,
                    rel,
                    Path::new(r"\\?\C:\DATA\projectslauncher\shared")
                ),
                abs("/new/ProjectsLauncher/shared")
            );
        }
    }

    #[test]
    fn targets_compare_by_where_they_lead() {
        let link = abs("/new/ProjectsLauncher/instances/a/minecraft/saves");
        assert!(same_target(
            &link,
            Path::new("../../b/minecraft/saves"),
            &abs("/new/ProjectsLauncher/instances/b/minecraft/saves")
        ));
        assert!(!same_target(&link, &abs("/worlds"), &abs("/other")));
        assert!(climbs_out(Path::new("a/b"), Path::new("../../x")));
        assert!(!climbs_out(Path::new("a/b"), Path::new("../x")));
        assert!(!climbs_out(Path::new("a/b"), Path::new("./c/../../x")));
    }
}
