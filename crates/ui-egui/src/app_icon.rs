//! The original lion with transparent surroundings and a palette for each interface theme.

pub fn png(dark: bool) -> &'static [u8] {
    if dark {
        include_bytes!("../../../assets/app-icon/pdfcraft-dark.png")
    } else {
        include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/ai.storyteller.pdfcraft.png")
    }
}

pub fn window_icon(dark: bool) -> Result<egui::IconData, image::ImageError> {
    let image = image::load_from_memory(png(dark))?.into_rgba8();
    Ok(egui::IconData { width: image.width(), height: image.height(), rgba: image.into_raw() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_icons_have_the_same_transparent_silhouette_and_opposite_palettes() {
        let light = window_icon(false).unwrap();
        let dark = window_icon(true).unwrap();
        assert_eq!((light.width, light.height), (dark.width, dark.height));
        assert_eq!(light.rgba[3], 0, "no surrounding tile");
        let mut changed = 0;
        for (a, b) in light.rgba.as_chunks::<4>().0.iter().zip(dark.rgba.as_chunks::<4>().0) {
            assert_eq!(a[3], b[3]);
            if a[3] == 255 {
                assert_eq!(u16::from(a[0]) + u16::from(b[0]), 250);
                changed += usize::from(a[0] != b[0]);
            }
        }
        assert!(changed > 1000, "lion follows the theme");
    }
}
