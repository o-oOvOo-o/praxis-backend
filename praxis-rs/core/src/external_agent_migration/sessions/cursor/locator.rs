use std::path::PathBuf;

const CURSOR_WORKSPACE_STORAGE: &str = "workspaceStorage";
const CURSOR_GLOBAL_STORAGE: &str = "globalStorage";

pub(super) struct CursorPaths {
    pub workspace_storage: PathBuf,
    pub global_db: PathBuf,
}

pub(super) fn locate_cursor_paths() -> Option<CursorPaths> {
    let user_dir = default_cursor_user_dir()?;
    if !user_dir.exists() {
        return None;
    }
    let workspace_storage = user_dir.join(CURSOR_WORKSPACE_STORAGE);
    let global_db = user_dir.join(CURSOR_GLOBAL_STORAGE).join("state.vscdb");
    if !workspace_storage.is_dir() || !global_db.is_file() {
        return None;
    }
    Some(CursorPaths {
        workspace_storage,
        global_db,
    })
}

fn default_cursor_user_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        return std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("Cursor").join("User"));
    }
    dirs::home_dir().map(|home| {
        if cfg!(target_os = "macos") {
            home.join("Library")
                .join("Application Support")
                .join("Cursor")
                .join("User")
        } else {
            home.join(".config").join("Cursor").join("User")
        }
    })
}
