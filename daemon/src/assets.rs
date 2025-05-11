use anyhow::Context;
use common::{image_data::ImageData, ipc::ResizeStrategy};
use resvg::usvg;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub struct AssetsManager {
    images: HashMap<Arc<str>, (ImageData, ResizeStrategy)>,
    fallback: Option<FallbackImage>,
}

pub enum FallbackImage {
    Color(image::Rgb<u8>),
    Image((ImageData, ResizeStrategy)),
    Svg(Vec<u8>),
}

impl From<(ImageData, ResizeStrategy)> for FallbackImage {
    fn from(value: (ImageData, ResizeStrategy)) -> Self {
        Self::Image(value)
    }
}

pub enum AssetUpdateMode {
    ReplaceAll,
    Single(Arc<str>),
}

impl AssetsManager {
    pub fn get(&self, name: &str, width: u32, height: u32) -> Option<(ImageData, ResizeStrategy)> {
        self.images.get(name).cloned().or_else(|| {
            self.fallback.as_ref().map(|fallback| match fallback {
                FallbackImage::Image(image) => (image.0.clone(), image.1),
                FallbackImage::Color(color) => {
                    let rgba_image = image::RgbaImage::from_pixel(
                        width,
                        height,
                        image::Rgba([color[0], color[1], color[2], 255]),
                    );
                    (ImageData::from(rgba_image), ResizeStrategy::No)
                }
                FallbackImage::Svg(svg_data) => {
                    let opt = usvg::Options::default();
                    let tree = usvg::Tree::from_data(svg_data, &opt).unwrap();
                    let mut pixmap = tiny_skia::Pixmap::new(width, height)
                        .context("Failed to create pixmap")
                        .unwrap();
                    let scale_x = width as f32 / tree.size().width();
                    let scale_y = height as f32 / tree.size().height();
                    resvg::render(
                        &tree,
                        tiny_skia::Transform::from_scale(scale_x, scale_y),
                        &mut pixmap.as_mut(),
                    );
                    let image = image::load_from_memory(&pixmap.encode_png().unwrap()).unwrap();
                    (ImageData::from(image), ResizeStrategy::No)
                }
            })
        })
    }

    pub fn insert<T>(&mut self, key: AssetUpdateMode, value: T)
    where
        T: Into<FallbackImage>,
    {
        match key {
            AssetUpdateMode::ReplaceAll => {
                self.images.clear();
                self.fallback = Some(value.into());
            }
            AssetUpdateMode::Single(key) => {
                if let FallbackImage::Image(image_data_with_strategy) = value.into() {
                    self.images.insert(key, image_data_with_strategy);
                }
            }
        }
    }
}
