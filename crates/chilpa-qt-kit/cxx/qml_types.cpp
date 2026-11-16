// The native bootstrap retains each primitive's registration object file.
extern "C" void chilpa_frame_fence_register_qml_type();
extern "C" void chilpa_page_lifetime_register_qml_type();
extern "C" void chilpa_playing_surface_register_qml_type();

extern "C" void chilpa_kit_register_qml_types()
{
    chilpa_frame_fence_register_qml_type();
    chilpa_page_lifetime_register_qml_type();
    chilpa_playing_surface_register_qml_type();
}
