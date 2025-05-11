use fast_image_resize::{self as fr, FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};
use image::{DynamicImage, RgbaImage};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImageData {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl ImageData {
    pub fn resize_stretch(self, width: u32, height: u32) -> anyhow::Result<Self> {
        let resized_img = if (self.width, self.height) != (width, height) {
            let src = fast_image_resize::images::ImageRef::new(
                self.width,
                self.height,
                &self.data,
                PixelType::U8x4,
            )?;

            let mut dst = fast_image_resize::images::Image::new(width, height, PixelType::U8x4);
            let mut resizer = Resizer::new();
            let options =
                ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));

            resizer.resize(&src, &mut dst, Some(&options))?;

            dst.into_vec()
        } else {
            self.data.clone()
        };

        Ok(Self {
            width,
            height,
            data: resized_img,
        })
    }

    pub fn resize_crop(self, width: u32, height: u32) -> anyhow::Result<Self> {
        let resized_img = if (self.width, self.height) != (width, height) {
            let src = fast_image_resize::images::ImageRef::new(
                self.width,
                self.height,
                &self.data,
                PixelType::U8x4,
            )?;

            let mut dst = fast_image_resize::images::Image::new(width, height, PixelType::U8x4);
            let mut resizer = Resizer::new();
            let options = ResizeOptions::new()
                .resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3))
                .fit_into_destination(Some((0.5, 0.5)));

            resizer.resize(&src, &mut dst, Some(&options))?;

            dst.into_vec()
        } else {
            self.data.clone()
        };

        Ok(Self {
            width,
            height,
            data: resized_img,
        })
    }

    pub fn resize_to_fit(self, width: u32, height: u32) -> anyhow::Result<Self> {
        if self.width == width && self.height == height {
            return Ok(self);
        }

        let mut src = fr::images::Image::from_vec_u8(
            self.width,
            self.height,
            self.data,
            fr::PixelType::U8x4,
        )?;

        let alpha_mul_div = fr::MulDiv::default();
        alpha_mul_div.multiply_alpha_inplace(&mut src)?;
        let mut dst = fr::images::Image::new(width, height, fr::PixelType::U8x4);
        let mut resizer = fr::Resizer::new();
        resizer.resize(&src, &mut dst, &ResizeOptions::default())?;
        alpha_mul_div.divide_alpha_inplace(&mut dst)?;

        Ok(Self {
            width: dst.width(),
            height: dst.height(),
            data: dst.into_vec(),
        })
    }

    pub fn pad(self, width: u32, height: u32, color: &[u8; 3]) -> Self {
        let channels = 4;

        let color = [color[0], color[1], color[2], 255];

        let mut padded = Vec::with_capacity((width * height * channels) as usize);

        let img = if self.width > width || self.height > height {
            let left = (self.width - width) / 2;
            let top = (self.height - height) / 2;
            self.crop(left, top, width, height)
        } else {
            self.crop(0, 0, width, height)
        };

        let (img_w, img_h) = (
            (img.width as usize).min(width as usize),
            (img.height as usize).min(height as usize),
        );

        (0..(((height as usize - img_h) / 2) * width as usize)).for_each(|_| {
            padded.extend_from_slice(&color);
        });

        // Calculate left and right border widths. `u32::div` rounds toward 0, so, if `img_w` is odd,
        // add an extra pixel to the right border to ensure the row is the correct width.
        let left_border_w = (width as usize - img_w) / 2;
        let right_border_w = left_border_w + (img_w % 2);

        (0..img_h).for_each(|row| {
            (0..left_border_w).for_each(|_| {
                padded.extend_from_slice(&color);
            });

            padded.extend_from_slice(
                &img.data
                    [(row * img_w * channels as usize)..((row + 1) * img_w * channels as usize)],
            );

            (0..right_border_w).for_each(|_| {
                padded.extend_from_slice(&color);
            });
        });

        while padded.len() < (height * width * channels) as usize {
            padded.extend_from_slice(&color);
        }

        Self {
            width,
            height,
            data: padded,
        }
    }

    pub fn crop(self, x: u32, y: u32, width: u32, height: u32) -> Self {
        if self.width == width && self.height == height {
            return self;
        }

        let x = x.min(self.width);
        let y = y.min(self.height);
        let width = width.min(self.width - x);
        let height = height.min(self.height - y);

        let mut data = Vec::with_capacity((width * height * 4) as usize);

        let begin = ((y * self.width) + x) * 4;
        let stride = self.width * 4;
        let row_size = width * 4;

        (0..height).for_each(|row_index| {
            let row = (begin + row_index * stride) as usize;
            data.extend_from_slice(&self.data[row..row + row_size as usize]);
        });

        Self {
            width,
            height,
            data,
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

impl From<DynamicImage> for ImageData {
    fn from(value: DynamicImage) -> Self {
        let rgba_image = value.to_rgba8();

        let width = rgba_image.width();
        let height = rgba_image.height();
        let data = rgba_image.as_raw().to_vec();

        Self {
            width,
            height,
            data,
        }
    }
}

impl From<RgbaImage> for ImageData {
    fn from(value: RgbaImage) -> Self {
        Self {
            width: value.width(),
            height: value.height(),
            data: value.as_raw().to_vec(),
        }
    }
}
