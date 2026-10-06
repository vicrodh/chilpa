//! Folders: the library's folders and their scan, browsing by folder, and
//! media opened without importing it. The window's crate depends on this
//! one, shows its screens and reads what a folder holds for its lists.

pub mod bridge {
    pub mod folder_list;
    pub mod folder_view;
    pub mod folders;
}
