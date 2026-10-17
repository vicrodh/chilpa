//! The dynamic background behind the whole window: the album-colored field,
//! the blurred cover, the desktop's wallpaper plain or blurred, and the
//! chrome that goes translucent over them. The window's crate depends on
//! this one; the facade is the one `chilpa-qt-kit` holds.

pub mod bridge {
    pub mod ambient;
}

mod demand;
