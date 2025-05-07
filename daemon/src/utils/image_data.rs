use fast_image_resize::{self as fr, ResizeOptions};
use image::DynamicImage;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImageData {
    width: u32,
    height: u32,
    rowstride: i32,
    has_alpha: bool,
    bits_per_sample: i32,
    channels: i32,
    data: Vec<u8>,
}

impl ImageData {
    pub fn into_rgba(self, max_size: u32) -> Self {
        let rgba = if self.has_alpha {
            self
        } else {
            let mut data = self.data;
            let mut new_data = Vec::with_capacity(data.len() / self.channels as usize * 4);

            data.chunks_exact_mut(self.channels as usize)
                .for_each(|chunk| {
                    new_data.extend_from_slice(chunk);
                    new_data.push(0xFF);
                });

            Self {
                has_alpha: true,
                data: new_data,
                channels: 4,
                rowstride: self.width as i32 * 4,
                ..self
            }
        };

        let mut src =
            fr::images::Image::from_vec_u8(rgba.width, rgba.height, rgba.data, fr::PixelType::U8x4)
                .unwrap();

        let alpha_mul_div = fr::MulDiv::default();
        alpha_mul_div.multiply_alpha_inplace(&mut src).unwrap();
        let mut dst = fr::images::Image::new(max_size, max_size, fr::PixelType::U8x4);
        let mut resizer = fr::Resizer::new();
        resizer
            .resize(&src, &mut dst, &ResizeOptions::default())
            .unwrap();
        alpha_mul_div.divide_alpha_inplace(&mut dst).unwrap();

        Self {
            width: dst.width(),
            height: dst.height(),
            data: dst.into_vec(),
            ..rgba
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }
}

impl TryFrom<DynamicImage> for ImageData {
    type Error = anyhow::Error;

    fn try_from(value: DynamicImage) -> Result<Self, Self::Error> {
        let rgba_image = value.to_rgba8();

        let width = rgba_image.width();
        let height = rgba_image.height();
        let data = rgba_image.as_raw().to_vec();

        let channels = 4;
        let bits_per_sample = 8;
        let has_alpha = true;
        let rowstride = (width * channels as u32) as i32;

        Ok(Self {
            width,
            height,
            rowstride,
            has_alpha,
            bits_per_sample,
            channels,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage, RgbaImage};

    #[test]
    fn converts_rgb_to_rgba() {
        let mut img = RgbImage::new(2, 2);
        img.put_pixel(0, 0, Rgb([255, 0, 0]));
        img.put_pixel(1, 0, Rgb([0, 255, 0]));
        img.put_pixel(0, 1, Rgb([0, 0, 255]));
        img.put_pixel(1, 1, Rgb([255, 255, 255]));

        let image_data = ImageData::try_from(DynamicImage::ImageRgb8(img)).unwrap();
        let converted = image_data.into_rgba(2);

        assert_eq!(converted.channels, 4);
        assert!(converted.has_alpha);
        assert_eq!(converted.data.len(), 2 * 2 * 4);
        assert_eq!(converted.rowstride, 2 * 4);
    }

    #[test]
    fn resizes_image_properly() {
        let img = RgbaImage::from_raw(4, 4, vec![255; 4 * 4 * 4]).unwrap();
        let image_data = ImageData::try_from(DynamicImage::ImageRgba8(img)).unwrap();

        let resized = image_data.into_rgba(2);

        assert_eq!(resized.width, 2);
        assert_eq!(resized.height, 2);
        assert_eq!(resized.rowstride, 2 * 2 * 4);
    }

    #[test]
    fn preserves_alpha_channel() {
        let mut img = RgbaImage::new(2, 2);
        img.put_pixel(0, 0, image::Rgba([255, 0, 0, 128]));
        let image_data = ImageData::try_from(DynamicImage::ImageRgba8(img)).unwrap();

        let converted = image_data.into_rgba(2);

        assert_eq!(converted.data[3], 128);
    }

    #[test]
    fn converts_from_dynamic_image() {
        let img = RgbaImage::new(32, 32);
        let image_data = ImageData::try_from(DynamicImage::ImageRgba8(img)).unwrap();

        assert_eq!(image_data.width, 32);
        assert_eq!(image_data.height, 32);
        assert_eq!(image_data.channels, 4);
        assert!(image_data.has_alpha);
        assert_eq!(image_data.bits_per_sample, 8);
        assert_eq!(image_data.rowstride, 32 * 4);
    }
}
