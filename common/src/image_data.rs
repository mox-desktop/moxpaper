use fast_image_resize::{self as fr, ResizeOptions};
use image::DynamicImage;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImageData {
    width: u32,
    height: u32,
    data: Vec<u8>,
}

impl ImageData {
    pub fn resize_to_fit(self, width: u32, height: u32) -> Self {
        if self.width == width && self.height == height {
            return self;
        }

        let mut src =
            fr::images::Image::from_vec_u8(self.width, self.height, self.data, fr::PixelType::U8x4)
                .unwrap();

        let alpha_mul_div = fr::MulDiv::default();
        alpha_mul_div.multiply_alpha_inplace(&mut src).unwrap();
        let mut dst = fr::images::Image::new(width, height, fr::PixelType::U8x4);
        let mut resizer = fr::Resizer::new();
        resizer
            .resize(&src, &mut dst, &ResizeOptions::default())
            .unwrap();
        alpha_mul_div.divide_alpha_inplace(&mut dst).unwrap();

        Self {
            width: dst.width(),
            height: dst.height(),
            data: dst.into_vec(),
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

//let svg_data =
//std::fs::read(path).context(format!("Failed to read SVG file: {}", path.display()))?;

//let opt = usvg::Options {
//resources_dir: Some(path.clone()),
//..usvg::Options::default()
//};

//let tree = usvg::Tree::from_data(&svg_data, &opt).context("Failed to parse SVG data")?;

//let mut pixmap =
//tiny_skia::Pixmap::new(width as u32, height as u32).context("Failed to create pixmap")?;

//let scale_x = width as f32 / tree.size().width();
//let scale_y = height as f32 / tree.size().height();

//resvg::render(
//&tree,
//tiny_skia::Transform::from_scale(scale_x, scale_y),
//&mut pixmap.as_mut(),
//);

//pixmap.encode_png().context("Failed to encode PNG")
