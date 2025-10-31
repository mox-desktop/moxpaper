use anyhow::Context;
use common::ipc::{
    BezierChoice, Data, Ipc, OutputInfo, ResizeStrategy, Transition, TransitionType, WallpaperData,
};
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
};

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

    /// Set the wallpaper source from an HTTP or HTTPS URL
    ///
    /// Downloads the image from the provided URL and converts it to wallpaper data.
    /// Supports optional authentication via HTTP headers.
    ///
    /// # Arguments
    /// * `url` - The HTTP/HTTPS URL to fetch the image from
    /// * `auth_headers` - Optional map of header name to header value for authentication
    ///   (e.g., `[("Authorization", "Bearer token")]`)
    ///
    /// # Example
    /// ```no_run
    /// # use libmoxpaper::MoxpaperClient;
    /// # let mut client = MoxpaperClient::connect().unwrap();
    /// client.set()
    ///     .http_url("https://example.com/image.jpg", None)
    ///     .apply()
    ///     .unwrap();
    /// ```
    pub fn http_url(
        self,
        url: impl Into<String>,
        auth_headers: Option<Vec<(String, String)>>,
    ) -> Self {
        let _url = url.into();
        let _auth_headers = auth_headers;

        unimplemented!("HTTP URL wallpaper fetching is not yet implemented")
    }

    /// Set the wallpaper source from an S3 bucket URL
    ///
    /// Fetches the image from an AWS S3 bucket using the provided credentials.
    ///
    /// # Arguments
    /// * `url` - S3 URL in format `s3://bucket-name/key` or `https://bucket.s3.region.amazonaws.com/key`
    /// * `access_key_id` - AWS access key ID
    /// * `secret_access_key` - AWS secret access key
    /// * `region` - AWS region (optional, can be inferred from URL or defaults to "us-east-1")
    ///
    /// # Example
    /// ```no_run
    /// # use libmoxpaper::MoxpaperClient;
    /// # let mut client = MoxpaperClient::connect().unwrap();
    /// client.set()
    ///     .s3_url(
    ///         "s3://my-bucket/wallpapers/image.jpg",
    ///         "AKIAIOSFODNN7EXAMPLE",
    ///         "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
    ///         Some("us-west-2"),
    ///     )
    ///     .apply()
    ///     .unwrap();
    /// ```
    pub fn s3_url(
        self,
        url: impl Into<String>,
        access_key_id: impl Into<String>,
        secret_access_key: impl Into<String>,
        region: Option<impl Into<String>>,
    ) -> Self {
        let _url = url.into();
        let _access_key_id = access_key_id.into();
        let _secret_access_key = secret_access_key.into();
        let _region = region.map(|r| r.into());

        unimplemented!("S3 URL wallpaper fetching is not yet implemented")
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
