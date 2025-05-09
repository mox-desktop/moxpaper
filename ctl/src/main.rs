use clap::Parser;
use common::{
    image_data::ImageData,
    ipc::{Data, Ipc, OutputInfo},
};
use resvg::usvg;
use std::{
    env,
    io::{BufRead, Write},
    path::PathBuf,
};

fn from_hex(hex: &str) -> Result<[u8; 3], String> {
    let chars = hex
        .chars()
        .filter(|&c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase() as u8);

    if chars.clone().count() != 6 {
        return Err(format!(
            "expected 6 characters, found {}",
            chars.clone().count()
        ));
    }

    let mut color = [0, 0, 0];

    for (i, c) in chars.enumerate() {
        match c {
            b'A'..=b'F' => color[i / 2] += c - b'A' + 10,
            b'0'..=b'9' => color[i / 2] += c - b'0',
            _ => {
                return Err(format!(
                    "expected [0-9], [a-f], or [A-F], found '{}'",
                    char::from(c)
                ));
            }
        }
        if i % 2 == 0 {
            color[i / 2] *= 16;
        }
    }
    Ok(color)
}

#[derive(Parser)]
pub struct Clear {
    #[arg(value_parser = from_hex, default_value = "000000")]
    pub color: [u8; 3],

    #[clap(short, long, default_value = "")]
    pub outputs: String,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
enum Cli {
    Img(Img),
    Clear(Clear),
}

#[derive(Parser)]
pub struct Img {
    #[arg(value_parser = parse_image)]
    pub image: CliImage,

    #[arg(short, long, value_delimiter = ',')]
    pub outputs: Vec<String>,
}

#[derive(Clone)]
pub enum CliImage {
    Path(PathBuf),
    Color([u8; 3]),
}

pub fn parse_image(raw: &str) -> Result<CliImage, String> {
    let path = PathBuf::from(raw);
    if raw == "-" || path.exists() {
        return Ok(CliImage::Path(path));
    }
    Err(format!("Path '{raw}' does not exist"))
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut socket_path =
        PathBuf::from(env::var("XDG_RUNTIME_DIR").expect("XDG_RUNTIME_DIR not set"));
    socket_path.push("mox/.moxpaper.sock");

    let Cli::Img(img) = cli else {
        return Ok(());
    };

    let ipc = Ipc::connect()?;

    let mut stream = ipc.get_stream();

    let mut buf = String::new();
    let mut reader = std::io::BufReader::new(&mut stream);
    reader.read_line(&mut buf)?;

    let outputs = serde_json::from_str::<Vec<OutputInfo>>(&buf);
    println!("{:?}", outputs);

    let CliImage::Path(path) = img.image else {
        return Ok(());
    };

    let image = if path.extension().is_some_and(|extension| extension == "svg") {
        let tree = {
            let opt = usvg::Options {
                resources_dir: Some(path.clone()),
                ..usvg::Options::default()
            };

            let svg_data = std::fs::read(path)?;
            usvg::Tree::from_data(&svg_data, &opt)?
        };

        let mut pixmap = tiny_skia::Pixmap::new(1920, 1080).unwrap();

        let scale_x = 1920. / tree.size().width();
        let scale_y = 1080. / tree.size().height();

        resvg::render(
            &tree,
            tiny_skia::Transform::from_scale(scale_x, scale_y),
            &mut pixmap.as_mut(),
        );

        image::load_from_memory(&pixmap.encode_png()?)
    } else {
        image::open(&path)
    }?;

    let image_data = ImageData::try_from(image)?.to_rgba().resize(1920, 1080);

    let data = Data {
        outputs: img.outputs,
        frames: vec![image_data.data().to_vec()],
    };

    let serialized = serde_json::to_string(&data)?;

    stream.write_all(serialized.as_bytes())?;

    println!("Image data sent successfully!");

    Ok(())
}
