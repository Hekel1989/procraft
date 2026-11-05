//! Pictures as they go to the server and come back: straight (not premultiplied) RGBA8, rows top to
//! bottom, sRGB.

use image::{GrayImage, ImageFormat, RgbaImage, imageops::FilterType};

use crate::Error;

#[derive(Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA8, `width × height × 4` bytes.
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Picture({} × {})", self.width, self.height)
    }
}

impl Picture {
    /// A picture of `rgba` (straight RGBA8); an error if it isn't `width × height` pixels.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Result<Picture, Error> {
        if width == 0 || height == 0 || rgba.len() as u64 != u64::from(width) * u64::from(height) * 4 {
            return Err(Error::picture(format!("{} bytes aren't {width} × {height} pixels", rgba.len())));
        }
        Ok(Picture { width, height, rgba })
    }

    /// The picture in a PNG or JPEG file's `bytes`.
    pub fn decode(bytes: &[u8]) -> Result<Picture, Error> {
        let image = image::load_from_memory(bytes).map_err(Error::picture)?.into_rgba8();
        let (width, height) = image.dimensions();
        Picture::new(width, height, image.into_raw())
    }

    /// The picture as a PNG file.
    pub fn to_png(&self) -> Result<Vec<u8>, Error> {
        let mut out = std::io::Cursor::new(Vec::new());
        self.image()?.write_to(&mut out, ImageFormat::Png).map_err(Error::picture)?;
        Ok(out.into_inner())
    }

    /// The picture scaled to `width × height` (Lanczos), its shape not kept.
    pub fn resized(&self, width: u32, height: u32) -> Result<Picture, Error> {
        if (width, height) == (self.width, self.height) {
            return Ok(self.clone());
        }
        let scaled = image::imageops::resize(&self.image()?, width.max(1), height.max(1), FilterType::Lanczos3);
        Picture::new(scaled.width(), scaled.height(), scaled.into_raw())
    }

    /// The part of the picture at `x`, `y`, `width × height` (canvas pixels).
    pub fn cropped(&self, x: u32, y: u32, width: u32, height: u32) -> Result<Picture, Error> {
        let right = x.checked_add(width).ok_or_else(|| Error::picture("the crop is off the picture"))?;
        let bottom = y.checked_add(height).ok_or_else(|| Error::picture("the crop is off the picture"))?;
        if width == 0 || height == 0 || right > self.width || bottom > self.height {
            return Err(Error::picture("the crop is off the picture"));
        }
        let stride = self.width as usize * 4;
        let row = width as usize * 4;
        let mut rgba = Vec::with_capacity(row * height as usize);
        for line in y..bottom {
            let at = (line as usize * stride) + x as usize * 4;
            let Some(part) = self.rgba.get(at..at + row) else {
                return Err(Error::picture("the crop is off the picture"));
            };
            rgba.extend_from_slice(part);
        }
        Picture::new(width, height, rgba)
    }

    /// The selection `mask` (`width × height`, one byte per pixel) scaled to `new_width ×
    /// new_height`, its soft edges kept.
    pub fn resized_mask(
        width: u32,
        height: u32,
        mask: &[u8],
        new_width: u32,
        new_height: u32,
    ) -> Result<Vec<u8>, Error> {
        let expected = u64::from(width) * u64::from(height);
        if width == 0 || height == 0 || mask.len() as u64 != expected {
            return Err(Error::picture(format!("{} bytes aren't a {width} × {height} mask", mask.len())));
        }
        if (width, height) == (new_width, new_height) {
            return Ok(mask.to_vec());
        }
        let image = GrayImage::from_raw(width, height, mask.to_vec())
            .ok_or_else(|| Error::picture("the mask doesn't match its size"))?;
        let scaled = image::imageops::resize(&image, new_width.max(1), new_height.max(1), FilterType::Lanczos3);
        Ok(scaled.into_raw())
    }

    /// A greyscale PNG of a selection `mask` (one byte per pixel, white selected), the size of the
    /// area it covers.
    pub fn mask_png(width: u32, height: u32, mask: &[u8]) -> Result<Vec<u8>, Error> {
        let expected = u64::from(width) * u64::from(height);
        if width == 0 || height == 0 || mask.len() as u64 != expected {
            return Err(Error::picture(format!("{} bytes aren't a {width} × {height} mask", mask.len())));
        }
        let image = GrayImage::from_raw(width, height, mask.to_vec())
            .ok_or_else(|| Error::picture("the mask doesn't match its size"))?;
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, ImageFormat::Png).map_err(Error::picture)?;
        Ok(out.into_inner())
    }

    /// The picture scaled down to fit in `side × side`, keeping its shape (as it is if it fits).
    pub fn thumbnail(&self, side: u32) -> Result<Picture, Error> {
        let scale = f64::from(side) / f64::from(self.width.max(self.height));
        if scale >= 1.0 {
            return Ok(self.clone());
        }
        let w = (f64::from(self.width) * scale).round().max(1.0) as u32;
        let h = (f64::from(self.height) * scale).round().max(1.0) as u32;
        self.resized(w, h)
    }

    /// The picture over `background` (RGB), with no transparency left.
    pub fn flattened(&self, background: [u8; 3]) -> Picture {
        let mut rgba = self.rgba.clone();
        for px in rgba.as_chunks_mut::<4>().0 {
            let a = u16::from(px[3]);
            for (c, &b) in px.iter_mut().zip(background.iter()) {
                *c = ((u16::from(*c) * a + u16::from(b) * (255 - a) + 127) / 255) as u8;
            }
            px[3] = 255;
        }
        Picture { width: self.width, height: self.height, rgba }
    }

    /// A control image from a layer's pixels (`premultiplied` RGBA8, `width × height`): the
    /// layer flattened onto white, its luminance inverted so dark strokes become white lines on
    /// black, which is what the control models expect.
    pub fn control(width: u32, height: u32, premultiplied: &[u8]) -> Result<Picture, Error> {
        if width == 0 || height == 0 || premultiplied.len() as u64 != u64::from(width) * u64::from(height) * 4 {
            return Err(Error::picture(format!(
                "{} bytes aren't {width} × {height} control pixels",
                premultiplied.len()
            )));
        }
        let mut rgba = Vec::with_capacity(premultiplied.len());
        for &[r, g, b, alpha] in premultiplied.as_chunks::<4>().0 {
            // over white: a premultiplied channel plus what the uncovered part of white adds
            let a = u32::from(alpha);
            let over = |c: u8| (u32::from(c) + 255 - a).min(255);
            let luma = (299 * over(r) + 587 * over(g) + 114 * over(b) + 500) / 1000;
            let line = u8::try_from(255u32.saturating_sub(luma)).unwrap_or(255);
            rgba.extend_from_slice(&[line, line, line, 255]);
        }
        Picture::new(width, height, rgba)
    }

    fn image(&self) -> Result<RgbaImage, Error> {
        RgbaImage::from_raw(self.width, self.height, self.rgba.clone())
            .ok_or_else(|| Error::picture("pixels don't match the size"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker(w: u32, h: u32) -> Picture {
        let rgba = (0..w * h)
            .flat_map(|i| if (i % w + i / w).is_multiple_of(2) { [255, 0, 0, 255] } else { [0, 0, 255, 128] });
        Picture::new(w, h, rgba.collect()).unwrap()
    }

    #[test]
    fn png_goes_both_ways() {
        let p = checker(7, 5);
        assert_eq!(Picture::decode(&p.to_png().unwrap()).unwrap(), p);
    }

    #[test]
    fn a_crop_holds_its_part_of_the_picture() {
        let p = Picture::new(3, 2, (0..6).flat_map(|i| [i, i, i, 255]).collect()).unwrap();
        let crop = p.cropped(1, 1, 2, 1).unwrap();
        assert_eq!((crop.width, crop.height), (2, 1));
        assert_eq!(crop.rgba, vec![4, 4, 4, 255, 5, 5, 5, 255]);
        assert!(p.cropped(2, 1, 2, 1).is_err());
        assert!(p.cropped(0, 0, 0, 1).is_err());
    }

    #[test]
    fn a_mask_is_a_greyscale_png() {
        let png = Picture::mask_png(2, 2, &[0, 128, 255, 64]).unwrap();
        let image = image::load_from_memory(&png).unwrap().into_luma8();
        assert_eq!((image.width(), image.height()), (2, 2));
        assert_eq!(image.into_raw(), vec![0, 128, 255, 64]);
        assert!(Picture::mask_png(2, 2, &[0, 1, 2]).is_err());
        let scaled = Picture::resized_mask(2, 2, &[0, 128, 255, 64], 4, 4).unwrap();
        assert_eq!(scaled.len(), 16);
        assert_eq!(Picture::resized_mask(2, 2, &[0, 128, 255, 64], 2, 2), Ok(vec![0, 128, 255, 64]));
    }

    #[test]
    fn sizes_are_checked_and_kept() {
        assert!(Picture::new(2, 2, vec![0; 15]).is_err());
        let p = checker(40, 20);
        assert_eq!((p.resized(10, 30).unwrap().width, p.resized(10, 30).unwrap().height), (10, 30));
        let t = p.thumbnail(8).unwrap();
        assert_eq!((t.width, t.height), (8, 4));
    }

    #[test]
    fn flattening_mixes_with_the_background() {
        let p = Picture::new(1, 1, vec![0, 0, 0, 0]).unwrap().flattened([255, 255, 255]);
        assert_eq!(p.rgba, vec![255, 255, 255, 255]);
        let half = Picture::new(1, 1, vec![0, 0, 0, 128]).unwrap().flattened([255, 255, 255]);
        assert_eq!(half.rgba, vec![127, 127, 127, 255]);
    }

    #[test]
    fn a_control_image_is_white_lines_on_black() {
        // premultiplied: an opaque black stroke, clear (white), and a half-covered black stroke
        let pixels = [0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 128];
        let control = Picture::control(3, 1, &pixels).unwrap();
        assert_eq!(control.rgba, vec![255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255]);
        assert!(Picture::control(2, 2, &pixels).is_err());
    }
}
