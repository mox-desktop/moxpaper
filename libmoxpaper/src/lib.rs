use anyhow::Context;
use common::ipc::{
    BezierChoice, Data, Ipc, OutputInfo, ResizeStrategy, Transition, TransitionType, WallpaperData,
};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

fn parse_s3_url(url: &str) -> anyhow::Result<(String, String)> {
    if let Some(stripped) = url.strip_prefix("s3://") {
        if let Some(slash_idx) = stripped.find('/') {
            let bucket = stripped[..slash_idx].to_string();
            let key = stripped[slash_idx + 1..].to_string();
            return Ok((bucket, key));
        }
        return Err(anyhow::anyhow!(
            "Invalid S3 URL: missing object key after bucket"
        ));
    }

    Err(anyhow::anyhow!(
        "Invalid S3 URL format. Expected s3://bucket/key"
    ))
}

/// Client for interacting with the moxpaper daemon
pub struct MoxpaperClient {
    ipc: Ipc<common::ipc::Client>,
    outputs: Vec<OutputInfo>,
}

/// Builder for configuring and setting wallpapers
pub struct WallpaperBuilder<'a> {
    client: &'a mut MoxpaperClient,
    data: Option<Data>,
    outputs: Vec<String>,
    resize: Option<ResizeStrategy>,
    transition: Option<Transition>,
}

impl<'a> WallpaperBuilder<'a> {
    /// Set the wallpaper source to a file path
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.data = Some(Data::Path(path.into()));
        self
    }

    /// Set the wallpaper source to raw image data
    pub fn image(mut self, image_data: common::image_data::ImageData) -> Self {
        self.data = Some(Data::Image(image_data));
        self
    }

    /// Set the wallpaper source to a solid color
    pub fn color(mut self, color: [u8; 3]) -> Self {
        self.data = Some(Data::Color(color));
        self
    }

    pub fn http_data(mut self, url: String, headers: Option<Vec<(String, String)>>) -> Self {
        self.data = Some(Data::Http { url, headers });
        self
    }

    pub fn s3_url<T>(
        mut self,
        url: T,
        access_key_id: T,
        secret_access_key: T,
        region: Option<T>,
        endpoint: Option<T>,
    ) -> Self
    where
        T: Into<String>,
    {
        let url = url.into();
        let (bucket, key) =
            parse_s3_url(&url).unwrap_or_else(|_| panic!("Failed to parse S3 URL: {}", url));

        let region = region.map(|r| r.into());
        let endpoint = endpoint.map(|e| e.into()).or_else(|| {
            std::env::var("MOXPAPER_S3_ENDPOINT").ok()
        });
        let access_key_id = access_key_id.into();
        let secret_access_key = secret_access_key.into();

        self.data = Some(Data::S3 {
            bucket,
            key,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
        });
        self
    }

    /// Set target outputs (empty vec means all outputs)
    pub fn outputs(mut self, outputs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.outputs = outputs.into_iter().map(|s| s.into()).collect();
        self
    }

    /// Set resize strategy
    pub fn resize(mut self, resize: ResizeStrategy) -> Self {
        self.resize = Some(resize);
        self
    }

    /// Set transition configuration
    pub fn transition(mut self, transition: Transition) -> Self {
        self.transition = Some(transition);
        self
    }

    /// Apply the wallpaper configuration
    pub fn apply(self) -> anyhow::Result<()> {
        let data = self
            .data
            .ok_or_else(|| anyhow::anyhow!("Wallpaper source not set"))?;
        let resize = self.resize.unwrap_or(ResizeStrategy::Crop);
        let transition = self.transition.unwrap_or_default();

        // For color, default to No resize strategy
        let resize = match &data {
            Data::Color(_) => ResizeStrategy::No,
            _ => resize,
        };

        self.client.send_wallpaper_data(WallpaperData {
            outputs: self.outputs.into_iter().map(|s| s.into()).collect(),
            data,
            resize,
            transition,
        })
    }
}

impl MoxpaperClient {
    /// Connect to the moxpaper daemon and retrieve output information
    pub fn connect() -> anyhow::Result<Self> {
        let ipc = Ipc::connect().context("Failed to connect to IPC")?;
        let mut ipc_stream = ipc.get_stream();

        // Read output information
        let mut buf = String::new();
        let mut ipc_reader = BufReader::new(&mut ipc_stream);
        ipc_reader.read_line(&mut buf)?;

        let outputs: Vec<OutputInfo> =
            serde_json::from_str(&buf).context("Failed to parse output information from daemon")?;

        Ok(Self { ipc, outputs })
    }

    /// Get information about all available outputs
    pub fn outputs(&self) -> &[OutputInfo] {
        &self.outputs
    }

    /// Create a builder for setting a wallpaper
    pub fn set(&mut self) -> WallpaperBuilder<'_> {
        WallpaperBuilder {
            client: self,
            data: None,
            outputs: Vec::new(),
            resize: None,
            transition: None,
        }
    }

    /// Helper method to send wallpaper data to the daemon
    fn send_wallpaper_data(&mut self, data: WallpaperData) -> anyhow::Result<()> {
        let mut stream = self.ipc.get_stream();
        let json = serde_json::to_string(&data).context("Failed to serialize wallpaper data")?;
        stream
            .write_all(json.as_bytes())
            .context("Failed to send wallpaper data to daemon")?;
        Ok(())
    }

    /// Build a transition configuration
    pub fn transition(
        transition_type: Option<TransitionType>,
        fps: Option<u64>,
        duration: Option<u128>,
        bezier: Option<BezierChoice>,
    ) -> Transition {
        Transition {
            transition_type,
            fps,
            duration,
            bezier,
        }
    }
}
