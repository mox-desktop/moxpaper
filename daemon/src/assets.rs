use anyhow::Context;
use libmoxpaper::{
    image_data::ImageData,
    ResizeStrategy, Transition,
};
use resvg::usvg;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub struct AssetsManager {
    images: HashMap<Arc<str>, AssetData>,
    fallback: Option<FallbackImage>,
}

#[derive(Clone)]
pub struct AssetData {
    pub image: ImageData,
    pub resize: ResizeStrategy,
    pub transition: Transition,
}

impl AssetData {
    pub fn new(image: ImageData, resize: ResizeStrategy, transition: Transition) -> Self {
        Self {
            image,
            resize,
            transition,
        }
    }
}

#[derive(Clone)]
pub enum FallbackImage {
    Color {
        color: image::Rgb<u8>,
        transition: Transition,
    },
    Image(AssetData),
    Svg {
        data: Box<[u8]>,
        transition: Transition,
    },
}

impl From<AssetData> for FallbackImage {
    fn from(value: AssetData) -> Self {
        Self::Image(value)
    }
}

impl From<(ImageData, ResizeStrategy, Transition)> for AssetData {
    fn from(value: (ImageData, ResizeStrategy, Transition)) -> Self {
        AssetData::new(value.0, value.1, value.2)
    }
}

impl AssetsManager {
    pub fn get(&self, name: &str, width: u32, height: u32) -> Option<AssetData> {
        self.images.get(name).cloned().or_else(|| {
            self.fallback.as_ref().map(|fallback| match fallback {
                FallbackImage::Image(asset_data) => asset_data.clone(),
                FallbackImage::Color { color, transition } => {
                    let rgba_image = image::RgbaImage::from_pixel(
                        width,
                        height,
                        image::Rgba([color[0], color[1], color[2], 255]),
                    );
                    AssetData::new(
                        ImageData::from(rgba_image),
                        ResizeStrategy::No,
                        transition.clone(),
                    )
                }
                FallbackImage::Svg { data, transition } => {
                    self.render_svg_fallback(data, width, height, transition)
                }
            })
        })
    }

    fn render_svg_fallback(
        &self,
        svg_data: &[u8],
        width: u32,
        height: u32,
        transition: &Transition,
    ) -> AssetData {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_data(svg_data, &opt)
            .context("Failed to parse SVG data")
            .unwrap_or_else(|_| panic!("Failed to parse SVG data"));

        let mut pixmap = tiny_skia::Pixmap::new(width, height)
            .context("Failed to create pixmap")
            .unwrap_or_else(|_| panic!("Failed to create pixmap"));

        let scale_x = width as f32 / tree.size().width();
        let scale_y = height as f32 / tree.size().height();

        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale_x, scale_y),
            &mut pixmap.as_mut(),
        );

        let png_data = pixmap
            .encode_png()
            .context("Failed to encode PNG")
            .unwrap_or_else(|_| panic!("Failed to encode PNG"));

        let image = image::load_from_memory(&png_data)
            .context("Failed to load image from memory")
            .unwrap_or_else(|_| panic!("Failed to load image from memory"));

        AssetData::new(
            ImageData::from(image),
            ResizeStrategy::No,
            transition.clone(),
        )
    }

    pub fn insert_asset(&mut self, key: Arc<str>, asset_data: AssetData) {
        self.images.insert(key, asset_data);
    }

    pub fn set_fallback(&mut self, fallback: FallbackImage) {
        self.fallback = Some(fallback);
    }
}
