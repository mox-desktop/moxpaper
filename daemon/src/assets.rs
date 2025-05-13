use anyhow::Context;
use common::{
    image_data::ImageData,
    ipc::{ResizeStrategy, Transition},
};
use resvg::usvg;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
pub struct AssetsManager {
    images: HashMap<Arc<str>, (ImageData, ResizeStrategy, Transition)>,
    fallback: Option<FallbackImage>,
}

pub enum FallbackImage {
    Color(image::Rgb<u8>, Transition),
    Image((ImageData, ResizeStrategy, Transition)),
    Svg(Box<[u8]>, Transition),
}

impl From<(ImageData, ResizeStrategy, Transition)> for FallbackImage {
    fn from(value: (ImageData, ResizeStrategy, Transition)) -> Self {
        Self::Image(value)
    }
}

pub enum AssetUpdateMode {
    ReplaceAll,
    Single(Arc<str>),
}

impl AssetsManager {
    pub fn get(
        &self,
        name: &str,
        width: u32,
        height: u32,
    ) -> Option<(ImageData, ResizeStrategy, Transition)> {
        self.images.get(name).cloned().or_else(|| {
            self.fallback.as_ref().map(|fallback| match fallback {
                FallbackImage::Image((img, resize, trans)) => (img.clone(), *resize, trans.clone()),
                FallbackImage::Color(color, trans) => {
                    let rgba_image = image::RgbaImage::from_pixel(
                        width,
                        height,
                        image::Rgba([color[0], color[1], color[2], 255]),
                    );
                    (
                        ImageData::from(rgba_image),
                        ResizeStrategy::No,
                        trans.clone(),
                    )
                }
                FallbackImage::Svg(svg_data, trans) => {
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
                    (ImageData::from(image), ResizeStrategy::No, trans.clone())
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
                if let FallbackImage::Image((img, resize, trans)) = value.into() {
                    self.images.insert(key, (img, resize, trans));
                }
            }
        }
    }
}
