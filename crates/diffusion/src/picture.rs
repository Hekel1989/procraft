//! Pictures as they go to the server and come back: straight (not premultiplied) RGBA8, rows top to
//! bottom, sRGB.

use image::{ImageFormat, RgbaImage, imageops::FilterType};

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
}
