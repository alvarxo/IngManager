#[cfg(test)]
mod tests {
    use std::fs;

    use crate::fs_util::{
        copy_recursively, ensure_not_into_self, is_engineering_model, move_entry,
        unique_destination,
    };
    use crate::path_security::{ensure_under_root, join_under_root};
    use tempfile::tempdir;

    #[test]
    fn external_drop_conflict_rename_and_engineering_flags() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let dest = root.join("2601").join("02_MODELOS");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("modelo.sdb"), b"existing").unwrap();

        let outside = root.join("desktop_drop");
        fs::create_dir_all(&outside).unwrap();
        let src_sdb = outside.join("modelo.sdb");
        let src_aux = outside.join("modelo.$2k");
        let src_folder = outside.join("lote_planos");
        fs::write(&src_sdb, b"new").unwrap();
        fs::write(&src_aux, b"aux").unwrap();
        fs::create_dir_all(&src_folder).unwrap();
        fs::write(src_folder.join("a.pdf"), b"pdf").unwrap();

        assert!(dest.join("modelo.sdb").exists());
        let planned = unique_destination(&dest, "modelo.sdb");
        assert_ne!(planned, dest.join("modelo.sdb"));
        assert!(is_engineering_model(&src_sdb));
        assert!(is_engineering_model(&src_aux));

        copy_recursively(&src_sdb, &planned).unwrap();
        copy_recursively(&src_aux, &dest.join("modelo.$2k")).unwrap();
        let folder_dest = dest.join("lote_planos");
        copy_recursively(&src_folder, &folder_dest).unwrap();
        assert!(planned.exists());
        assert!(dest.join("modelo.$2k").exists());
        assert!(folder_dest.join("a.pdf").exists());
        // Originals remain for copy-default semantics
        assert!(src_sdb.exists());
    }

    #[test]
    fn external_drop_move_outside_copies_then_removes_source() {
        let dir = tempdir().unwrap();
        let dest = dir.path().join("dest");
        fs::create_dir_all(&dest).unwrap();
        let src = dir.path().join("from_explorer.txt");
        fs::write(&src, b"hello").unwrap();
        let target = dest.join("from_explorer.txt");
        copy_recursively(&src, &target).unwrap();
        fs::remove_file(&src).unwrap();
        assert!(target.exists());
        assert!(!src.exists());
    }

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
