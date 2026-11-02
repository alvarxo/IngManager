#[cfg(test)]
mod tests {
    use std::fs;

    use crate::fs_util::{
        copy_recursively, ensure_not_into_self, move_entry, unique_destination,
    };
    use crate::path_security::{ensure_under_root, join_under_root};
    use tempfile::tempdir;

    #[test]
    fn copy_move_and_trash_slot_stay_under_root() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let project = root.join("2601").join("02_MODELOS");
        fs::create_dir_all(&project).unwrap();
        let src = project.join("modelo.rvt");
        fs::write(&src, b"rvt").unwrap();

        let dest_dir = project.join("backup");
        fs::create_dir_all(&dest_dir).unwrap();
        let dest = unique_destination(&dest_dir, "modelo.rvt");
        copy_recursively(&src, &dest).unwrap();
        assert!(dest.exists());

        let moved = dest_dir.join("modelo-moved.rvt");
        move_entry(&dest, &moved).unwrap();
        assert!(moved.exists());
        assert!(!dest.exists());

        ensure_under_root(root, &moved).unwrap();
        let trash = join_under_root(root, ".ingmanager/trash/uuid1/modelo-moved.rvt").unwrap();
        fs::create_dir_all(trash.parent().unwrap()).unwrap();
        move_entry(&moved, &trash).unwrap();
        assert!(trash.exists());
        ensure_under_root(root, &trash).unwrap();

        assert!(ensure_not_into_self(&project, &project.join("nested")).is_err()
            || {
                fs::create_dir_all(project.join("nested")).unwrap();
                ensure_not_into_self(&project, &project.join("nested")).is_err()
            });
    }
}
